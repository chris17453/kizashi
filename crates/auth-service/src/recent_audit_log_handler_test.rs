//! Tests for `GET /v1/audit-log` (`get_recent_audit_log`) — the general, chronological
//! cross-entity audit trail added alongside the existing entity-scoped `GET
//! /v1/audit-log/:entity_id` (`get_user_audit_log`, covered in `user_handlers_test.rs`). Split
//! into its own file per CLAUDE.md §0.6/§0.7 rather than growing `user_handlers_test.rs` past the
//! 500-line limit; shares that file's `default_state`/`send` helpers rather than duplicating
//! them. The `X-Internal-Secret` gate itself (this route sitting in the protected router group)
//! is covered separately in `lib_test.rs`, which is the file that already exercises
//! `build_router` end to end.

use super::user_handlers_test::{default_state, send};
use super::*;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::audit_log::{AuditLogEntry, ChangeType};
use axum::routing::get;
use axum::Router;
use chrono::Duration;
use std::sync::Arc;

fn router(state: AuthState) -> Router {
    Router::new().route("/v1/audit-log", get(get_recent_audit_log)).with_state(state)
}

fn entry_at(tenant_id: Uuid, changed_at: chrono::DateTime<Utc>) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "local_user".to_string(),
        entity_id: Uuid::new_v4(),
        change_type: ChangeType::Created,
        actor: "tester".to_string(),
        before: None,
        after: serde_json::json!({"username": "bob"}),
        changed_at,
    }
}

#[tokio::test]
async fn returns_entries_for_the_callers_tenant_most_recent_first() {
    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let reader = Arc::new(InMemoryAuditLogReader::default());
    reader.entries.lock().unwrap().push(entry_at(tenant_id, now - Duration::minutes(2)));
    reader.entries.lock().unwrap().push(entry_at(tenant_id, now));
    reader.entries.lock().unwrap().push(entry_at(other_tenant_id, now - Duration::minutes(1)));
    let state = AuthState { audit_log_reader: reader, ..default_state() };

    let response = send(
        router(state),
        "GET",
        "/v1/audit-log".to_string(),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let entries = json.as_array().unwrap();
    assert_eq!(entries.len(), 2);
    // most-recent-first: the `now` entry sorts before the `now - 2m` entry.
    let first_changed_at = entries[0]["changed_at"].as_str().unwrap();
    let second_changed_at = entries[1]["changed_at"].as_str().unwrap();
    assert!(first_changed_at > second_changed_at);
}

#[tokio::test]
async fn a_small_explicit_limit_is_honored() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let reader = Arc::new(InMemoryAuditLogReader::default());
    for i in 0..5 {
        reader.entries.lock().unwrap().push(entry_at(tenant_id, now - Duration::minutes(i)));
    }
    let state = AuthState { audit_log_reader: reader, ..default_state() };

    let response = send(
        router(state),
        "GET",
        "/v1/audit-log?limit=2".to_string(),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn before_cursor_excludes_entries_at_or_after_that_timestamp() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let cursor = now - Duration::minutes(1);
    let reader = Arc::new(InMemoryAuditLogReader::default());
    reader.entries.lock().unwrap().push(entry_at(tenant_id, now)); // at-or-after cursor: excluded
    reader.entries.lock().unwrap().push(entry_at(tenant_id, cursor)); // == cursor: excluded
    reader.entries.lock().unwrap().push(entry_at(tenant_id, now - Duration::minutes(2))); // before: included
    let state = AuthState { audit_log_reader: reader, ..default_state() };

    let response = send(
        router(state),
        "GET",
        // `+` in the RFC3339 UTC offset (`+00:00`) is a reserved query-string character (would
        // decode as a space), so percent-encode it — same as any real client must.
        format!("/v1/audit-log?before={}", cursor.to_rfc3339().replace('+', "%2B")),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_caller_from_a_different_tenant_never_sees_another_tenants_entries() {
    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();
    let reader = Arc::new(InMemoryAuditLogReader::default());
    reader.entries.lock().unwrap().push(entry_at(other_tenant_id, Utc::now()));
    let state = AuthState { audit_log_reader: reader, ..default_state() };

    let response = send(
        router(state),
        "GET",
        "/v1/audit-log".to_string(),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn missing_tenant_header_returns_401() {
    let response = send(
        router(default_state()),
        "GET",
        "/v1/audit-log".to_string(),
        None,
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
