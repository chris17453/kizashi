//! Config/Admin Service (spec §6, service #11): CRUD + immutable audit logging for
//! TriggerDefinition and NormalizationMapping in v1 (ADR-0010). Every admin/config mutation
//! writes an audit_log row in the same transaction as the entity change (CLAUDE.md §5).

mod action_template_handlers;
mod action_template_repository;
mod analysis_config_handlers;
mod analysis_config_publisher;
mod analysis_config_repository;
mod app_definition_handlers;
mod app_definition_repository;
mod audit_log;
mod data_source_handlers;
mod data_source_repository;
mod encryption;
mod event_type_definition_repository;
mod handlers;
mod health;
mod internal_secret;
mod mapping_publisher;
mod normalization_mapping_repository;
mod pipeline_definition_handlers;
mod pipeline_definition_repository;
mod report_run_repository;
mod saved_search_query_handlers;
mod saved_search_query_repository;
mod sensor_handlers;
mod sensor_publisher;
mod sensor_repository;
mod trigger_definition_repository;
mod trigger_publisher;

pub use action_template_handlers::{
    create_action_template, delete_action_template, get_action_template, list_action_templates,
    update_action_template, ActionTemplateState,
};
pub use action_template_repository::{
    ActionTemplateRepository, ActionTemplateRepositoryError, PostgresActionTemplateRepository,
};
pub use analysis_config_handlers::{get_analysis_config, put_analysis_config, AnalysisConfigState};
pub use analysis_config_publisher::{
    AnalysisConfigPublishError, AnalysisConfigPublisher, RabbitMqAnalysisConfigPublisher,
};
pub use analysis_config_repository::{
    AnalysisConfigRepository, AnalysisConfigRepositoryError, PostgresAnalysisConfigRepository,
};
pub use app_definition_handlers::{
    create_app_definition, get_app_definition, list_app_definitions, update_app_definition,
    AppDefinitionState,
};
pub use app_definition_repository::{
    AppDefinitionRepository, AppDefinitionRepositoryError, PostgresAppDefinitionRepository,
};
pub use audit_log::{
    record_audit_entry, AuditLogEntry, AuditLogError, AuditLogReader, ChangeType,
    PostgresAuditLogReader,
};
pub use data_source_handlers::{
    create_data_source, delete_data_source, get_data_source, list_data_sources, update_data_source,
    DataSourceState,
};
pub use data_source_repository::{
    DataSourceRepository, DataSourceRepositoryError, PostgresDataSourceRepository,
};
pub use encryption::{ApiKeyEncryptor, EncryptionError};
pub use event_type_definition_repository::{
    EventTypeDefinitionRepository, EventTypeDefinitionRepositoryError,
    PostgresEventTypeDefinitionRepository,
};
pub use handlers::{
    create_event_type, create_event_type_version, create_mapping, create_report_run,
    create_trigger, delete_mapping, delete_trigger, get_audit_log, get_event_type, get_mapping,
    get_recent_audit_log, get_trigger, list_event_types, list_mappings, list_report_runs,
    list_triggers, update_mapping, update_report_run, update_trigger, AdminState,
};
pub use health::healthz;
pub use internal_secret::require_internal_secret;
pub use mapping_publisher::{MappingPublishError, MappingPublisher, RabbitMqMappingPublisher};
pub use normalization_mapping_repository::{
    NormalizationMappingRepository, NormalizationMappingRepositoryError,
    PostgresNormalizationMappingRepository,
};
pub use pipeline_definition_handlers::{
    create_pipeline_definition, delete_pipeline_definition, get_pipeline_definition,
    list_pipeline_definitions, update_pipeline_definition, PipelineDefinitionState,
};
pub use pipeline_definition_repository::{
    PipelineDefinitionRepository, PipelineDefinitionRepositoryError,
    PostgresPipelineDefinitionRepository,
};
pub use report_run_repository::{
    PostgresReportRunRepository, ReportRunRepository, ReportRunRepositoryError,
};
pub use saved_search_query_handlers::{
    create_saved_search_query, delete_saved_search_query, list_saved_search_queries,
    update_saved_search_query, SavedSearchQueryState,
};
pub use saved_search_query_repository::{
    PostgresSavedSearchQueryRepository, SavedSearchQueryRepository, SavedSearchQueryRepositoryError,
};
pub use sensor_handlers::{
    create_sensor, delete_sensor, get_sensor, get_sensor_by_name, list_sensors, update_sensor,
    SensorState,
};
pub use sensor_publisher::{RabbitMqSensorPublisher, SensorPublishError, SensorPublisher};
pub use sensor_repository::{PostgresSensorRepository, SensorRepository, SensorRepositoryError};
pub use trigger_definition_repository::{
    PostgresTriggerDefinitionRepository, TriggerDefinitionRepository,
    TriggerDefinitionRepositoryError,
};
pub use trigger_publisher::{RabbitMqTriggerPublisher, TriggerPublishError, TriggerPublisher};

use axum::routing::{get, post};
use axum::Router;

#[allow(clippy::too_many_arguments)]
pub fn build_router(
    state: AdminState,
    sensor_state: SensorState,
    analysis_config_state: AnalysisConfigState,
    saved_search_query_state: SavedSearchQueryState,
    action_template_state: ActionTemplateState,
    data_source_state: DataSourceState,
    pipeline_definition_state: PipelineDefinitionState,
    app_definition_state: AppDefinitionState,
    internal_secret: String,
) -> Router {
    let admin_routes = Router::new()
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
        .route("/v1/audit-log", get(get_recent_audit_log))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .route("/v1/event-type-definitions", post(create_event_type).get(list_event_types))
        .route("/v1/event-type-definitions/:id", get(get_event_type))
        .route("/v1/event-type-definitions/:id/versions", post(create_event_type_version))
        .route("/v1/report-runs", post(create_report_run).get(list_report_runs))
        .route("/v1/report-runs/:id", axum::routing::put(update_report_run))
        .with_state(state);

    let sensor_routes = Router::new()
        .route("/v1/sensors", post(create_sensor).get(list_sensors))
        .route("/v1/sensors/by-name/:name", get(get_sensor_by_name))
        .route("/v1/sensors/:id", get(get_sensor).put(update_sensor).delete(delete_sensor))
        .with_state(sensor_state);

    let analysis_config_routes = Router::new()
        .route("/v1/analysis-config", get(get_analysis_config).put(put_analysis_config))
        .with_state(analysis_config_state);

    let saved_search_query_routes = Router::new()
        .route(
            "/v1/saved-search-queries",
            post(create_saved_search_query).get(list_saved_search_queries),
        )
        .route(
            "/v1/saved-search-queries/:id",
            axum::routing::put(update_saved_search_query).delete(delete_saved_search_query),
        )
        .with_state(saved_search_query_state);

    let action_template_routes = Router::new()
        .route("/v1/action-templates", post(create_action_template).get(list_action_templates))
        .route(
            "/v1/action-templates/:id",
            get(get_action_template).put(update_action_template).delete(delete_action_template),
        )
        .with_state(action_template_state);
    let data_source_routes = Router::new()
        .route("/v1/data-sources", post(create_data_source).get(list_data_sources))
        .route(
            "/v1/data-sources/:id",
            get(get_data_source).put(update_data_source).delete(delete_data_source),
        )
        .with_state(data_source_state);
    let pipeline_definition_routes = Router::new()
        .route(
            "/v1/pipeline-definitions",
            post(create_pipeline_definition).get(list_pipeline_definitions),
        )
        .route(
            "/v1/pipeline-definitions/:id",
            get(get_pipeline_definition)
                .put(update_pipeline_definition)
                .delete(delete_pipeline_definition),
        )
        .with_state(pipeline_definition_state);
    let app_definition_routes = Router::new()
        .route("/v1/app-definitions", post(create_app_definition).get(list_app_definitions))
        .route("/v1/app-definitions/:id", get(get_app_definition).put(update_app_definition))
        .with_state(app_definition_state);

    let protected_routes = admin_routes
        .merge(sensor_routes)
        .merge(analysis_config_routes)
        .merge(saved_search_query_routes)
        .merge(action_template_routes)
        .merge(app_definition_routes)
        .merge(data_source_routes)
        .merge(pipeline_definition_routes)
        .layer(axum::middleware::from_fn_with_state(internal_secret, require_internal_secret));

    let healthz_route = Router::new().route("/healthz", get(healthz));

    protected_routes.merge(healthz_route)
}
