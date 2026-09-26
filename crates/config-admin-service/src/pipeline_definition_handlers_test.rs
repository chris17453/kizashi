use super::*;
use crate::data_source_repository::data_source_repository_test::InMemoryDataSourceRepository;
use crate::data_source_repository::DataSourceRepository;
use crate::pipeline_definition_repository::pipeline_definition_repository_test::InMemoryPipelineDefinitionRepository;
use common::{DataSource, DataSourceKind, DataSourceMode, PipelineMode};
use std::sync::Arc;

#[test]
fn validation_rejects_an_invalid_pipeline_definition() {
    let value = PipelineDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: String::new(),
        description: String::new(),
        data_source_id: Uuid::new_v4(),
        target_object_type_id: None,
        mode: common::PipelineMode::Projection,
        steps: serde_json::json!([]),
        enabled: true,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(validate(&value).is_err());
}

fn pipeline(tenant_id: Uuid, data_source_id: Uuid) -> PipelineDefinition {
    PipelineDefinition {
        id: Uuid::nil(),
        tenant_id,
        name: "Orders projection".to_string(),
        description: String::new(),
        data_source_id,
        target_object_type_id: None,
        mode: PipelineMode::Projection,
        steps: serde_json::json!([{"kind":"extract","config":{}}]),
        enabled: true,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn data_source(tenant_id: Uuid, id: Uuid) -> DataSource {
    DataSource {
        id,
        tenant_id,
        name: "ERP".to_string(),
        description: String::new(),
        kind: DataSourceKind::Api,
        mode: DataSourceMode::Read,
        connection: serde_json::json!({}),
        credential_ref: None,
        enabled: true,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn operator_headers(tenant_id: Uuid) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", tenant_id.to_string().parse().unwrap());
    headers.insert("x-role", "operator".parse().unwrap());
    headers.insert("x-username", "test.operator".parse().unwrap());
    headers
}

#[tokio::test]
async fn create_rejects_a_data_source_owned_by_another_tenant() {
    let tenant_id = Uuid::new_v4();
    let source_tenant_id = Uuid::new_v4();
    let data_source_id = Uuid::new_v4();
    let data_sources = Arc::new(InMemoryDataSourceRepository::default());
    data_sources.create(data_source(source_tenant_id, data_source_id), "test").await.unwrap();
    let state = PipelineDefinitionState {
        repository: Arc::new(InMemoryPipelineDefinitionRepository::default()),
        data_source_repository: data_sources,
    };
    let response = create_pipeline_definition(
        State(state),
        operator_headers(tenant_id),
        Json(pipeline(tenant_id, data_source_id)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_accepts_a_data_source_owned_by_the_request_tenant() {
    let tenant_id = Uuid::new_v4();
    let data_source_id = Uuid::new_v4();
    let data_sources = Arc::new(InMemoryDataSourceRepository::default());
    data_sources.create(data_source(tenant_id, data_source_id), "test").await.unwrap();
    let state = PipelineDefinitionState {
        repository: Arc::new(InMemoryPipelineDefinitionRepository::default()),
        data_source_repository: data_sources,
    };
    let response = create_pipeline_definition(
        State(state),
        operator_headers(tenant_id),
        Json(pipeline(tenant_id, data_source_id)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}
