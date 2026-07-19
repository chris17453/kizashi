use super::*;
use crate::archive_store::archive_store_test::InMemoryArchiveStore;
use crate::audit_log::audit_log_test::{FailingAuditLogReader, InMemoryAuditLogReader};
use crate::raw_record_client::raw_record_client_test::InMemoryRawRecordClient;
use crate::retention_policy::retention_policy_test::{
    FailingRetentionPolicyRepository, InMemoryRetentionPolicyRepository,
};
use crate::retention_policy::DataClass;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/retention-policies", post(create_policy).get(list_policies))
        .route("/v1/retention-policies/:id", get(get_policy).put(update_policy))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state)
}

fn sample_policy(tenant_id: Uuid) -> RetentionPolicy {
    RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days: 90,
        enabled: true,
    }
}

fn default_state() -> AppState {
    AppState {
        policy_repository: Arc::new(InMemoryRetentionPolicyRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        record_client: Arc::new(InMemoryRawRecordClient::default()),
        archive_store: Arc::new(InMemoryArchiveStore::default()),
    }
}

async fn send(
    app: Router,
    method: &str,
    uri: String,
    tenant_header: Option<Uuid>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(tenant_id) = tenant_header {
        req = req.header("x-tenant-id", tenant_id.to_string()).header("x-role", "admin");
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_policy_succeeds_when_tenant_matches() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_policy_rejects_a_tenant_mismatch() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_policy_requires_tenant_header() {
    let policy = sample_policy(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "POST",
        "/v1/retention-policies".to_string(),
        None,
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_policy_requires_role_header() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/retention-policies")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::from(serde_json::to_value(&policy).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_policy_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/retention-policies")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .body(Body::from(serde_json::to_value(&policy).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_policy_allows_an_operator_role() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/retention-policies")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::from(serde_json::to_value(&policy).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn get_policy_returns_404_for_unknown_id() {
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/retention-policies/{}", Uuid::new_v4()),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_policy_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/retention-policies/{}", policy.id),
        Some(tenant_id),
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_policies_returns_backend_error_as_500() {
    let mut state = default_state();
    state.policy_repository = Arc::new(FailingRetentionPolicyRepository);
    let response = send(
        router(state),
        "GET",
        "/v1/retention-policies".to_string(),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn full_policy_crud_round_trip() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let app_state = default_state();

    let create = send(
        router(app_state.clone()),
        "POST",
        "/v1/retention-policies".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&policy).unwrap()),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);

    let get = send(
        router(app_state.clone()),
        "GET",
        format!("/v1/retention-policies/{}", policy.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(get.status(), StatusCode::OK);

    let list = send(
        router(app_state.clone()),
        "GET",
        "/v1/retention-policies".to_string(),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(list.into_body(), usize::MAX).await.unwrap();
    let policies: Vec<RetentionPolicy> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(policies.len(), 1);

    let mut updated = policy.clone();
    updated.enabled = false;
    let update = send(
        router(app_state),
        "PUT",
        format!("/v1/retention-policies/{}", policy.id),
        Some(tenant_id),
        Some(serde_json::to_value(&updated).unwrap()),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_audit_log_returns_entries_scoped_to_tenant_and_entity() {
    let tenant_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(crate::audit_log::AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "retention_policy".to_string(),
        entity_id,
        change_type: crate::audit_log::ChangeType::Created,
        actor: tenant_id.to_string(),
        before: None,
        after: serde_json::json!({}),
        changed_at: Utc::now(),
    });
    let mut state = default_state();
    state.audit_reader = Arc::new(reader);

    let response =
        send(router(state), "GET", format!("/v1/audit-log/{entity_id}"), Some(tenant_id), None)
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn get_audit_log_returns_500_on_backend_failure() {
    let mut state = default_state();
    state.audit_reader = Arc::new(FailingAuditLogReader);

    let response = send(
        router(state),
        "GET",
        format!("/v1/audit-log/{}", Uuid::new_v4()),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn get_audit_log_requires_tenant_header() {
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/audit-log/{}", Uuid::new_v4()),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
