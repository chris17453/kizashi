//! Handler tests for both audit-log read endpoints: `get_audit_log` (single entity,
//! chronological ascending) and `get_recent_audit_log` (cross-entity, most-recent-first, the
//! general compliance "show me every admin action" feed added alongside it). Split out of
//! `handlers_test.rs` because that file was already at the 500-line limit (CLAUDE.md §0.6).

use super::*;
use crate::audit_log::audit_log_test::{FailingAuditLogReader, InMemoryAuditLogReader};
use crate::audit_log::{AuditLogEntry, ChangeType};
use crate::mapping_publisher::mapping_publisher_test::InMemoryMappingPublisher;
use crate::normalization_mapping_repository::normalization_mapping_repository_test::InMemoryNormalizationMappingRepository;
use crate::trigger_definition_repository::trigger_definition_repository_test::InMemoryTriggerDefinitionRepository;
use crate::trigger_publisher::trigger_publisher_test::InMemoryTriggerPublisher;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use chrono::{Duration, Utc};
use tower::ServiceExt;

fn router(state: AdminState) -> Router {
    Router::new()
        .route("/v1/audit-log", get(get_recent_audit_log))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state)
}

fn state_with_reader(reader: impl AuditLogReader + 'static) -> AdminState {
    AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(reader),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
    }
}

async fn send(app: Router, uri: String, tenant_header: Option<Uuid>) -> axum::http::Response<Body> {
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(tenant_id) = tenant_header {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

fn entry_at(tenant_id: Uuid, entity_id: Uuid, changed_at: chrono::DateTime<Utc>) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "trigger_definition".to_string(),
        entity_id,
        change_type: ChangeType::Created,
        actor: "test-actor@example.com".to_string(),
        before: None,
        after: serde_json::json!({}),
        changed_at,
    }
}

#[tokio::test]
async fn get_audit_log_returns_entries_scoped_to_tenant_and_entity() {
    let tenant_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(entry_at(tenant_id, entity_id, Utc::now()));
    let state = state_with_reader(reader);

    let response = send(router(state), format!("/v1/audit-log/{entity_id}"), Some(tenant_id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn get_audit_log_returns_500_on_backend_failure() {
    let state = state_with_reader(FailingAuditLogReader);

    let response =
        send(router(state), format!("/v1/audit-log/{}", Uuid::new_v4()), Some(Uuid::new_v4()))
            .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn get_audit_log_requires_tenant_header() {
    let state = state_with_reader(InMemoryAuditLogReader::default());
    let response = send(router(state), format!("/v1/audit-log/{}", Uuid::new_v4()), None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_recent_audit_log_returns_entries_for_callers_tenant_most_recent_first() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(entry_at(
        tenant_id,
        Uuid::new_v4(),
        now - Duration::seconds(20),
    ));
    reader.entries.lock().unwrap().push(entry_at(tenant_id, Uuid::new_v4(), now));
    reader.entries.lock().unwrap().push(entry_at(
        tenant_id,
        Uuid::new_v4(),
        now - Duration::seconds(10),
    ));
    let state = state_with_reader(reader);

    let response = send(router(state), "/v1/audit-log".to_string(), Some(tenant_id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<AuditLogEntry> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries[0].changed_at >= entries[1].changed_at);
    assert!(entries[1].changed_at >= entries[2].changed_at);
}

#[tokio::test]
async fn get_recent_audit_log_default_limit_truncates_results() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let reader = InMemoryAuditLogReader::default();
    for i in 0..3 {
        reader.entries.lock().unwrap().push(entry_at(
            tenant_id,
            Uuid::new_v4(),
            now - Duration::seconds(i),
        ));
    }
    let state = state_with_reader(reader);

    let response = send(router(state), "/v1/audit-log?limit=2".to_string(), Some(tenant_id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<AuditLogEntry> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn get_recent_audit_log_before_cursor_excludes_entries_at_or_after_it() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let cursor_time = now - Duration::seconds(10);
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(entry_at(tenant_id, Uuid::new_v4(), now)); // after cursor
    reader.entries.lock().unwrap().push(entry_at(tenant_id, Uuid::new_v4(), cursor_time)); // at cursor
    reader.entries.lock().unwrap().push(entry_at(
        tenant_id,
        Uuid::new_v4(),
        now - Duration::seconds(20),
    )); // before cursor
    let state = state_with_reader(reader);

    // `to_rfc3339()` includes `+`/`:` which must be percent-encoded in a query string, or axum's
    // `Query` extractor will fail to parse the timestamp (`+` decodes to a space otherwise).
    let encoded_cursor = cursor_time.to_rfc3339().replace('+', "%2B").replace(':', "%3A");
    let response =
        send(router(state), format!("/v1/audit-log?before={encoded_cursor}"), Some(tenant_id))
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<AuditLogEntry> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].changed_at < cursor_time);
}

#[tokio::test]
async fn get_recent_audit_log_never_returns_another_tenants_entries() {
    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(entry_at(tenant_id, Uuid::new_v4(), Utc::now()));
    reader.entries.lock().unwrap().push(entry_at(other_tenant_id, Uuid::new_v4(), Utc::now()));
    let state = state_with_reader(reader);

    let response = send(router(state), "/v1/audit-log".to_string(), Some(tenant_id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<AuditLogEntry> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tenant_id, tenant_id);
}

#[tokio::test]
async fn get_recent_audit_log_requires_tenant_header() {
    let state = state_with_reader(InMemoryAuditLogReader::default());
    let response = send(router(state), "/v1/audit-log".to_string(), None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_recent_audit_log_returns_500_on_backend_failure() {
    let state = state_with_reader(FailingAuditLogReader);
    let response = send(router(state), "/v1/audit-log".to_string(), Some(Uuid::new_v4())).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
