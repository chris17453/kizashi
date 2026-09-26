use crate::action_template_repository::action_template_repository_test::InMemoryActionTemplateRepository;
use crate::analysis_config_publisher::analysis_config_publisher_test::InMemoryAnalysisConfigPublisher;
use crate::analysis_config_repository::analysis_config_repository_test::InMemoryAnalysisConfigRepository;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::data_source_repository::data_source_repository_test::InMemoryDataSourceRepository;
use crate::mapping_publisher::mapping_publisher_test::InMemoryMappingPublisher;
use crate::normalization_mapping_repository::normalization_mapping_repository_test::InMemoryNormalizationMappingRepository;
use crate::pipeline_definition_repository::pipeline_definition_repository_test::InMemoryPipelineDefinitionRepository;
use crate::saved_search_query_repository::saved_search_query_repository_test::InMemorySavedSearchQueryRepository;
use crate::sensor_publisher::sensor_publisher_test::InMemorySensorPublisher;
use crate::sensor_repository::sensor_repository_test::InMemorySensorRepository;
use crate::trigger_definition_repository::trigger_definition_repository_test::InMemoryTriggerDefinitionRepository;
use crate::trigger_publisher::trigger_publisher_test::InMemoryTriggerPublisher;
use crate::{
    build_router, ActionTemplateState, AdminState, AnalysisConfigState, AppDefinitionRepository,
    AppDefinitionRepositoryError, DataSourceState, SavedSearchQueryState, SensorState,
};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

const TEST_SECRET: &str = "test-internal-secret";
#[derive(Default)]
struct InMemoryAppDefinitionRepository;
#[async_trait]
impl AppDefinitionRepository for InMemoryAppDefinitionRepository {
    async fn create(
        &self,
        value: common::AppDefinition,
        _: &str,
    ) -> Result<common::AppDefinition, AppDefinitionRepositoryError> {
        Ok(value)
    }
    async fn update(
        &self,
        value: common::AppDefinition,
        _: &str,
    ) -> Result<common::AppDefinition, AppDefinitionRepositoryError> {
        Ok(value)
    }
    async fn get(
        &self,
        _: uuid::Uuid,
        _: uuid::Uuid,
    ) -> Result<Option<common::AppDefinition>, AppDefinitionRepositoryError> {
        Ok(None)
    }
    async fn list(
        &self,
        _: uuid::Uuid,
    ) -> Result<Vec<common::AppDefinition>, AppDefinitionRepositoryError> {
        Ok(vec![])
    }
}

fn test_router() -> Router {
    let admin_state = AdminState {
        trigger_repository: Arc::new(InMemoryTriggerDefinitionRepository::default()),
        mapping_repository: Arc::new(InMemoryNormalizationMappingRepository::default()),
        audit_reader: Arc::new(InMemoryAuditLogReader::default()),
        trigger_publisher: Arc::new(InMemoryTriggerPublisher::default()),
        mapping_publisher: Arc::new(InMemoryMappingPublisher::default()),
        event_type_repository: None,
        report_run_repository: None,
    };
    let sensor_state = SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::default()),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };
    let analysis_config_state = AnalysisConfigState {
        repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        publisher: Arc::new(InMemoryAnalysisConfigPublisher::default()),
    };
    let saved_search_query_state = SavedSearchQueryState {
        saved_search_query_repository: Arc::new(InMemorySavedSearchQueryRepository::default()),
    };
    let action_template_state =
        ActionTemplateState { repository: Arc::new(InMemoryActionTemplateRepository::default()) };

    let data_source_repository = Arc::new(InMemoryDataSourceRepository::default());
    build_router(
        admin_state,
        sensor_state,
        analysis_config_state,
        saved_search_query_state,
        action_template_state,
        DataSourceState { repository: data_source_repository.clone() },
        crate::PipelineDefinitionState {
            repository: Arc::new(InMemoryPipelineDefinitionRepository::default()),
            data_source_repository,
        },
        crate::AppDefinitionState { repository: Arc::new(InMemoryAppDefinitionRepository) },
        TEST_SECRET.to_string(),
    )
}

#[tokio::test]
async fn protected_route_without_internal_secret_returns_401() {
    let app = test_router();

    let response = app
        .oneshot(Request::builder().uri("/v1/trigger-definitions").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn healthz_succeeds_with_zero_headers() {
    let app = test_router();

    let response =
        app.oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
