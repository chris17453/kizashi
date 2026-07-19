use super::*;
use crate::audit_log::audit_log_test::{FailingAuditLogReader, InMemoryAuditLogReader};
use crate::normalization_mapping_repository::normalization_mapping_repository_test::{
    FailingNormalizationMappingRepository, InMemoryNormalizationMappingRepository,
};
use crate::trigger_definition_repository::trigger_definition_repository_test::{
    FailingTriggerDefinitionRepository, InMemoryTriggerDefinitionRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use common::TriggerCondition;
use std::collections::BTreeMap;
use tower::ServiceExt;

fn router(state: AdminState) -> Router {
    Router::new()
        .route("/v1/trigger-definitions", post(create_trigger).get(list_triggers))
        .route("/v1/trigger-definitions/:id", get(get_trigger).put(update_trigger))
        .route("/v1/normalization-mappings", post(create_mapping).get(list_mappings))
        .route("/v1/normalization-mappings/:id", get(get_mapping).put(update_mapping))
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
        req = req.header("x-tenant-id", tenant_id.to_string());
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
async fn get_audit_log_returns_entries_scoped_to_tenant_and_entity() {
    let tenant_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    let reader = InMemoryAuditLogReader::default();
    reader.entries.lock().unwrap().push(crate::audit_log::AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "trigger_definition".to_string(),
        entity_id,
        change_type: crate::audit_log::ChangeType::Created,
        actor: tenant_id.to_string(),
        before: None,
        after: serde_json::json!({}),
        changed_at: Utc::now(),
    });
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(reader),
    };

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
    let state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(FailingAuditLogReader),
    };

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
    let state = default_state();
    let response =
        send(router(state), "GET", format!("/v1/audit-log/{}", Uuid::new_v4()), None, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
