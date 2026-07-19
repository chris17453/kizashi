use super::*;
use crate::agent_status_client::agent_status_client_test::InMemoryAgentStatusClient;
use crate::api_key_store::api_key_store_test::{FailingApiKeyStore, InMemoryApiKeyStore};
use crate::api_key_store::ApiKeyStore;
use crate::rate_limiter::{RateLimiter, SystemClock};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{delete, get};
use axum::Router;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn state_with(api_key_store: Arc<dyn crate::ApiKeyStore>) -> GatewayState {
    GatewayState {
        api_key_store,
        audit_reader: Arc::new(crate::audit_log::audit_log_test::InMemoryAuditLogReader::default()),
        rate_limiter: Arc::new(RateLimiter::new(
            600,
            Duration::from_secs(60),
            Box::new(SystemClock),
        )),
        http_client: reqwest::Client::new(),
        ingestion_service_url: "http://localhost:0".to_string(),
        agent_status_client: Arc::new(InMemoryAgentStatusClient::default()),
    }
}

fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/api-keys", get(list_api_keys).post(create_api_key))
        .route("/v1/api-keys/:id", delete(revoke_api_key))
        .route("/v1/api-keys/:id/audit-log", get(get_api_key_audit_log))
        .with_state(state)
}

#[tokio::test]
async fn create_api_key_returns_the_plaintext_key_once() {
    let state = state_with(Arc::new(InMemoryApiKeyStore::default()));
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/api-keys")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"label":"ci-agent"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["label"], "ci-agent");
    assert!(body["api_key"].as_str().unwrap().starts_with("kzsh_"));
}

#[tokio::test]
async fn create_api_key_requires_role_header() {
    let state = state_with(Arc::new(InMemoryApiKeyStore::default()));
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/api-keys")
                .header("x-tenant-id", tenant_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(r#"{"label":"ci-agent"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_api_key_rejects_a_viewer_role() {
    let state = state_with(Arc::new(InMemoryApiKeyStore::default()));
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/api-keys")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"label":"ci-agent"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_api_key_missing_tenant_header_is_unauthorized() {
    let state = state_with(Arc::new(InMemoryApiKeyStore::default()));

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/api-keys")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"label":"ci-agent"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_api_keys_is_scoped_to_tenant_and_never_exposes_key_material() {
    let store = Arc::new(InMemoryApiKeyStore::default());
    let tenant_id = Uuid::new_v4();
    store.create(tenant_id, "mine").await.unwrap();
    store.create(Uuid::new_v4(), "not-mine").await.unwrap();
    let state = state_with(store);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/api-keys")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let keys = body.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["label"], "mine");
    assert!(keys[0].get("key_hash").is_none());
    assert!(keys[0].get("api_key").is_none());
}

#[tokio::test]
async fn revoke_api_key_marks_it_revoked() {
    let store = Arc::new(InMemoryApiKeyStore::default());
    let tenant_id = Uuid::new_v4();
    let (summary, _plaintext) = store.create(tenant_id, "to-revoke").await.unwrap();
    let state = state_with(store.clone());

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/api-keys/{}", summary.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let listed = store.list(tenant_id).await.unwrap();
    assert!(listed[0].revoked_at.is_some());
}

#[tokio::test]
async fn get_api_key_audit_log_returns_the_created_entry() {
    let store = Arc::new(InMemoryApiKeyStore::default());
    let tenant_id = Uuid::new_v4();
    let (summary, _plaintext) = store.create(tenant_id, "audited").await.unwrap();
    let mut state = state_with(store);
    let reader = crate::audit_log::audit_log_test::InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(crate::audit_log::AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "api_key".to_string(),
        entity_id: summary.id,
        change_type: crate::audit_log::ChangeType::Created,
        actor: tenant_id.to_string(),
        before: None,
        after: serde_json::json!({"label": "audited"}),
        changed_at: chrono::Utc::now(),
    });
    state.audit_reader = Arc::new(reader);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/api-keys/{}/audit-log", summary.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["change_type"], "created");
}

#[tokio::test]
async fn backend_failure_surfaces_as_500() {
    let state = state_with(Arc::new(FailingApiKeyStore));
    let tenant_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/api-keys")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
