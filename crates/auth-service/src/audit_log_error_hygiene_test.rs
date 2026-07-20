use super::*;
use crate::audit_log::audit_log_test::FailingAuditLogReader;
use crate::session_audit_writer::session_audit_writer_test::FailingSessionAuditWriter;
use crate::user_handlers::user_handlers_test::{default_state, send};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/users/:id/audit-log", get(get_user_audit_log))
        .route("/v1/audit-log", get(get_recent_audit_log))
        .route("/v1/audit-log/session-revoked", post(post_session_revoked_audit))
        .with_state(state)
}

#[tokio::test]
async fn get_user_audit_log_backend_failure_does_not_leak_the_raw_error() {
    let tenant_id = Uuid::new_v4();
    let state = AuthState { audit_log_reader: Arc::new(FailingAuditLogReader), ..default_state() };

    let response = send(
        router(state),
        "GET",
        format!("/v1/users/{}/audit-log", Uuid::new_v4()),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}

#[tokio::test]
async fn get_recent_audit_log_backend_failure_does_not_leak_the_raw_error() {
    let tenant_id = Uuid::new_v4();
    let state = AuthState { audit_log_reader: Arc::new(FailingAuditLogReader), ..default_state() };

    let response =
        send(router(state), "GET", "/v1/audit-log".to_string(), Some(tenant_id), None, None).await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}

#[tokio::test]
async fn post_session_revoked_audit_backend_failure_does_not_leak_the_raw_error() {
    let tenant_id = Uuid::new_v4();
    let state =
        AuthState { session_audit_writer: Arc::new(FailingSessionAuditWriter), ..default_state() };

    let response = send(
        router(state),
        "POST",
        "/v1/audit-log/session-revoked".to_string(),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"session_id": Uuid::new_v4(), "revoked_username": "bob"})),
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}
