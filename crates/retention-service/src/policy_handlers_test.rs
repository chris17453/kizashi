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

pub(crate) const TEST_INTERNAL_SECRET: &str = "test-internal-secret";

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(crate::healthz))
        .route("/v1/retention-policies", post(create_policy).get(list_policies))
        .route(
            "/v1/retention-policies/:id",
            get(get_policy).put(update_policy).delete(delete_policy),
        )
        .route("/v1/audit-log", get(get_recent_audit_log))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state)
}

/// Minimal percent-encoding for an RFC3339 timestamp used as a query-string value in tests —
/// `:` and `+` are reserved/special in a query string (`+` decodes to a space), so an unescaped
/// `2024-01-01T00:00:00+00:00` would otherwise be mangled before axum's `Query` extractor ever
/// sees it.
pub(crate) fn url_encode_rfc3339(value: &str) -> String {
    value.replace(':', "%3A").replace('+', "%2B")
}

pub(crate) fn sample_audit_entry(
    tenant_id: Uuid,
    changed_at: chrono::DateTime<Utc>,
) -> crate::audit_log::AuditLogEntry {
    crate::audit_log::AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "retention_policy".to_string(),
        entity_id: Uuid::new_v4(),
        change_type: crate::audit_log::ChangeType::Created,
        actor: tenant_id.to_string(),
        before: None,
        after: serde_json::json!({}),
        changed_at,
    }
}

pub(crate) fn sample_policy(tenant_id: Uuid) -> RetentionPolicy {
    RetentionPolicy {
        id: Uuid::new_v4(),
        tenant_id,
        data_class: DataClass::Raw,
        ttl_days: 90,
        enabled: true,
    }
}

pub(crate) fn default_state() -> AppState {
    AppState {
        policy_repository: Arc::new(InMemoryRetentionPolicyRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        record_client: Arc::new(InMemoryRawRecordClient::default()),
        archive_store: Arc::new(InMemoryArchiveStore::default()),
        internal_secret: TEST_INTERNAL_SECRET.to_string(),
        hold_repository: None,
    }
}

/// Always-valid `X-Internal-Secret`, and — when `tenant_header` is `Some` — always-valid
/// `X-Tenant-Id`/`X-Role`/`X-Username` too. `policy_handlers_auth_test.rs` uses `send_raw`
/// instead when it needs to omit or vary an individual header.
pub(crate) async fn send(
    app: Router,
    method: &str,
    uri: String,
    tenant_header: Option<Uuid>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut headers = vec![("x-internal-secret", TEST_INTERNAL_SECRET.to_string())];
    if let Some(tenant_id) = tenant_header {
        headers.push(("x-tenant-id", tenant_id.to_string()));
        headers.push(("x-role", "admin".to_string()));
        headers.push(("x-username", "test-user".to_string()));
    }
    send_raw(app, method, uri, &headers, body).await
}

/// Sends a request with exactly the headers given — no implicit extras.
pub(crate) async fn send_raw(
    app: Router,
    method: &str,
    uri: String,
    headers: &[(&str, String)],
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    for (name, value) in headers {
        req = req.header(*name, value.as_str());
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
async fn update_policy_rejects_a_tenant_mismatch() {
    let policy = sample_policy(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/retention-policies/{}", policy.id),
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
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

#[tokio::test]
async fn get_recent_audit_log_returns_entries_for_tenant_most_recent_first() {
    let tenant_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    let now = Utc::now();
    let older = sample_audit_entry(tenant_id, now - chrono::Duration::seconds(10));
    let newer = sample_audit_entry(tenant_id, now);
    reader.entries.lock().unwrap().push(older.clone());
    reader.entries.lock().unwrap().push(newer.clone());
    // A different tenant's entry must never leak into this tenant's trail.
    reader.entries.lock().unwrap().push(sample_audit_entry(Uuid::new_v4(), now));
    let mut state = default_state();
    state.audit_reader = Arc::new(reader);

    let response =
        send(router(state), "GET", "/v1/audit-log".to_string(), Some(tenant_id), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["id"], serde_json::json!(newer.id));
    assert_eq!(entries[1]["id"], serde_json::json!(older.id));
}

#[tokio::test]
async fn get_recent_audit_log_honors_a_small_limit() {
    let tenant_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    let now = Utc::now();
    for i in 0..5 {
        reader
            .entries
            .lock()
            .unwrap()
            .push(sample_audit_entry(tenant_id, now - chrono::Duration::seconds(i)));
    }
    let mut state = default_state();
    state.audit_reader = Arc::new(reader);

    let response =
        send(router(state), "GET", "/v1/audit-log?limit=2".to_string(), Some(tenant_id), None)
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn get_recent_audit_log_before_cursor_excludes_entries_at_or_after() {
    let tenant_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    let now = Utc::now();
    let old = sample_audit_entry(tenant_id, now - chrono::Duration::seconds(20));
    let cursor_entry = sample_audit_entry(tenant_id, now - chrono::Duration::seconds(10));
    let new = sample_audit_entry(tenant_id, now);
    reader.entries.lock().unwrap().push(old.clone());
    reader.entries.lock().unwrap().push(cursor_entry.clone());
    reader.entries.lock().unwrap().push(new.clone());
    let mut state = default_state();
    state.audit_reader = Arc::new(reader);

    let before = url_encode_rfc3339(&cursor_entry.changed_at.to_rfc3339());
    let response =
        send(router(state), "GET", format!("/v1/audit-log?before={before}"), Some(tenant_id), None)
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], serde_json::json!(old.id));
}

#[tokio::test]
async fn get_recent_audit_log_never_returns_another_tenants_entries() {
    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(sample_audit_entry(other_tenant_id, Utc::now()));
    let mut state = default_state();
    state.audit_reader = Arc::new(reader);

    let response =
        send(router(state), "GET", "/v1/audit-log".to_string(), Some(tenant_id), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entries.len(), 0);
}

#[tokio::test]
async fn get_recent_audit_log_requires_tenant_header() {
    let response =
        send(router(default_state()), "GET", "/v1/audit-log".to_string(), None, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_recent_audit_log_returns_500_on_backend_failure() {
    let mut state = default_state();
    state.audit_reader = Arc::new(FailingAuditLogReader);

    let response =
        send(router(state), "GET", "/v1/audit-log".to_string(), Some(Uuid::new_v4()), None).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}

#[tokio::test]
async fn delete_policy_succeeds_then_get_returns_404() {
    let tenant_id = Uuid::new_v4();
    let policy = sample_policy(tenant_id);
    let state = AppState {
        policy_repository: Arc::new(InMemoryRetentionPolicyRepository::with_policy(policy.clone())),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        record_client: Arc::new(InMemoryRawRecordClient::default()),
        archive_store: Arc::new(InMemoryArchiveStore::default()),
        internal_secret: TEST_INTERNAL_SECRET.to_string(),
        hold_repository: None,
    };
    let app = router(state);

    let delete_response = send(
        app.clone(),
        "DELETE",
        format!("/v1/retention-policies/{}", policy.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_response =
        send(app, "GET", format!("/v1/retention-policies/{}", policy.id), Some(tenant_id), None)
            .await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_policy_returns_404_for_unknown_id() {
    let response = send(
        router(default_state()),
        "DELETE",
        format!("/v1/retention-policies/{}", Uuid::new_v4()),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
