use super::*;
use crate::session_audit_writer::session_audit_writer_test::{
    FailingSessionAuditWriter, InMemorySessionAuditWriter,
};
use crate::user_handlers::user_handlers_test::{default_state, send};
use axum::routing::post;
use axum::Router;
use std::sync::Arc;

fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/audit-log/session-revoked", post(post_session_revoked_audit))
        .with_state(state)
}

#[tokio::test]
async fn records_the_revocation_for_an_admin() {
    let tenant_id = Uuid::new_v4();
    let writer = Arc::new(InMemorySessionAuditWriter::default());
    let mut state = default_state();
    state.session_audit_writer = writer.clone();
    let session_id = Uuid::new_v4();

    let response = send(
        router(state),
        "POST",
        "/v1/audit-log/session-revoked".to_string(),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"session_id": session_id, "revoked_username": "bob"})),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let recorded = writer.recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, tenant_id);
    assert_eq!(recorded[0].1, "test-actor");
    assert_eq!(recorded[0].2, session_id);
    assert_eq!(recorded[0].3, "bob");
}

#[tokio::test]
async fn requires_admin_role() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "POST",
        "/v1/audit-log/session-revoked".to_string(),
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({"session_id": Uuid::new_v4(), "revoked_username": "bob"})),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn requires_a_username_header() {
    let tenant_id = Uuid::new_v4();
    let response = crate::user_handlers::user_handlers_test::send_as(
        router(default_state()),
        "POST",
        "/v1/audit-log/session-revoked".to_string(),
        Some(tenant_id),
        Some("admin"),
        None,
        Some(serde_json::json!({"session_id": Uuid::new_v4(), "revoked_username": "bob"})),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn surfaces_a_backend_failure() {
    let tenant_id = Uuid::new_v4();
    let mut state = default_state();
    state.session_audit_writer = Arc::new(FailingSessionAuditWriter);

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
}
