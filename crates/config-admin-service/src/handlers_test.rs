use super::*;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::mapping_publisher::mapping_publisher_test::InMemoryMappingPublisher;
use crate::normalization_mapping_repository::normalization_mapping_repository_test::{
    FailingNormalizationMappingRepository, InMemoryNormalizationMappingRepository,
};
use crate::trigger_definition_repository::trigger_definition_repository_test::{
    FailingTriggerDefinitionRepository, InMemoryTriggerDefinitionRepository,
};
use crate::trigger_publisher::trigger_publisher_test::InMemoryTriggerPublisher;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use common::TriggerCondition;
use std::collections::BTreeMap;
use tower::ServiceExt;

fn router(state: AdminState) -> Router {
    Router::new()
        .route("/v1/trigger-definitions", post(create_trigger).get(list_triggers))
        .route(
            "/v1/trigger-definitions/:id",
            get(get_trigger).put(update_trigger).delete(delete_trigger),
        )
        .route("/v1/normalization-mappings", post(create_mapping).get(list_mappings))
        .route(
            "/v1/normalization-mappings/:id",
            get(get_mapping).put(update_mapping).delete(delete_mapping),
        )
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state)
}

fn sample_trigger(tenant_id: Uuid) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "test".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 3 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

fn sample_mapping(tenant_id: Uuid) -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping::new(tenant_id, "ticket", field_map)
}

fn default_state() -> AdminState {
    AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
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
        req = req
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", "admin")
            .header("x-username", "test-actor@example.com");
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_trigger_succeeds_when_tenant_matches() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/trigger-definitions".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&trigger).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_trigger_rejects_a_tenant_mismatch() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/trigger-definitions".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&trigger).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn update_trigger_rejects_a_tenant_mismatch() {
    // tenant_mismatch runs before the repository lookup, so no pre-existing trigger is needed
    // to prove the header/body tenant check itself rejects the request.
    let trigger = sample_trigger(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/trigger-definitions/{}", trigger.id),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&trigger).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_trigger_requires_tenant_header() {
    let trigger = sample_trigger(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "POST",
        "/v1/trigger-definitions".to_string(),
        None,
        Some(serde_json::to_value(&trigger).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_trigger_requires_role_header() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/trigger-definitions")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::from(serde_json::to_value(&trigger).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Regression coverage for the audit-actor bug (CLAUDE.md §5): `X-Username` is required on
/// every write handler that records an audit-log entry, mirroring how `X-Tenant-Id`/`X-Role`
/// are already required above — without it, `actor` on the audit row has no real identity to
/// record.
#[tokio::test]
async fn create_trigger_requires_username_header() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/trigger-definitions")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::from(serde_json::to_value(&trigger).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_trigger_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/trigger-definitions")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .body(Body::from(serde_json::to_value(&trigger).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_trigger_allows_an_operator_role() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/trigger-definitions")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "test-actor@example.com")
                .body(Body::from(serde_json::to_value(&trigger).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_mapping_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/normalization-mappings")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .body(Body::from(serde_json::to_value(&mapping).unwrap().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_trigger_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/trigger-definitions/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_trigger_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/trigger-definitions/{}", trigger.id),
        Some(tenant_id),
        Some(serde_json::to_value(&trigger).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_triggers_returns_backend_error_as_500() {
    let state = AdminState {
        trigger_repository: Arc::new(FailingTriggerDefinitionRepository),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let response = send(
        router(state),
        "GET",
        "/v1/trigger-definitions".to_string(),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn delete_trigger_succeeds_then_get_returns_404() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::with_trigger(
            trigger.clone(),
        )),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let app = router(state);

    let delete_response = send(
        app.clone(),
        "DELETE",
        format!("/v1/trigger-definitions/{}", trigger.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_response =
        send(app, "GET", format!("/v1/trigger-definitions/{}", trigger.id), Some(tenant_id), None)
            .await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_trigger_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "DELETE",
        format!("/v1/trigger-definitions/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_trigger_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::with_trigger(
            trigger.clone(),
        )),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/trigger-definitions/{}", trigger.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn full_trigger_crud_round_trip() {
    let tenant_id = Uuid::new_v4();
    let trigger = sample_trigger(tenant_id);
    let app_state = default_state();

    let create = send(
        router(app_state.clone()),
        "POST",
        "/v1/trigger-definitions".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&trigger).unwrap()),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);

    let get = send(
        router(app_state.clone()),
        "GET",
        format!("/v1/trigger-definitions/{}", trigger.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(get.status(), StatusCode::OK);

    let list = send(
        router(app_state.clone()),
        "GET",
        "/v1/trigger-definitions".to_string(),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(list.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let triggers: Vec<TriggerDefinition> =
        serde_json::from_value(body["triggers"].clone()).unwrap();
    assert_eq!(triggers.len(), 1);

    let mut updated = trigger.clone();
    updated.enabled = false;
    let update = send(
        router(app_state),
        "PUT",
        format!("/v1/trigger-definitions/{}", trigger.id),
        Some(tenant_id),
        Some(serde_json::to_value(&updated).unwrap()),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_mapping_succeeds_when_tenant_matches() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/normalization-mappings".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&mapping).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_mapping_rejects_a_tenant_mismatch() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/normalization-mappings".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&mapping).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn update_mapping_rejects_a_tenant_mismatch() {
    let mapping = sample_mapping(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/normalization-mappings/{}", mapping.id),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&mapping).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_mapping_returns_404_for_unknown_id() {
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/normalization-mappings/{}", Uuid::new_v4()),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_mappings_returns_backend_error_as_500() {
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(FailingNormalizationMappingRepository),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let response = send(
        router(state),
        "GET",
        "/v1/normalization-mappings".to_string(),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn full_mapping_crud_round_trip() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let app_state = default_state();

    let create = send(
        router(app_state.clone()),
        "POST",
        "/v1/normalization-mappings".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&mapping).unwrap()),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);

    let get = send(
        router(app_state.clone()),
        "GET",
        format!("/v1/normalization-mappings/{}", mapping.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(get.status(), StatusCode::OK);

    let mut updated = mapping.clone();
    updated.version = 2;
    let update = send(
        router(app_state),
        "PUT",
        format!("/v1/normalization-mappings/{}", mapping.id),
        Some(tenant_id),
        Some(serde_json::to_value(&updated).unwrap()),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_mapping_succeeds_then_get_returns_404() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::with_mapping(
            mapping.clone(),
        )),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let app = router(state);

    let delete_response = send(
        app.clone(),
        "DELETE",
        format!("/v1/normalization-mappings/{}", mapping.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_response = send(
        app,
        "GET",
        format!("/v1/normalization-mappings/{}", mapping.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_mapping_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "DELETE",
        format!("/v1/normalization-mappings/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_mapping_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::with_mapping(
            mapping.clone(),
        )),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/normalization-mappings/{}", mapping.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// get_audit_log / get_recent_audit_log handler tests live in `audit_log_handlers_test.rs` —
// this file was already at the 500-line limit (CLAUDE.md §0.6), so the audit-log-specific
// handler tests get their own file rather than pushing this one further over.
