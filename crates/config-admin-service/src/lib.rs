//! Config/Admin Service (spec §6, service #11): CRUD + immutable audit logging for
//! TriggerDefinition and NormalizationMapping in v1 (ADR-0010). Every admin/config mutation
//! writes an audit_log row in the same transaction as the entity change (CLAUDE.md §5).

mod agent_handlers;
mod agent_repository;
mod audit_log;
mod handlers;
mod health;
mod normalization_mapping_repository;
mod trigger_definition_repository;
mod trigger_publisher;

pub use agent_handlers::{
    create_agent, delete_agent, get_agent, get_agent_by_name, list_agents, update_agent, AgentState,
};
pub use agent_repository::{AgentRepository, AgentRepositoryError, PostgresAgentRepository};
pub use audit_log::{
    record_audit_entry, AuditLogEntry, AuditLogError, AuditLogReader, ChangeType,
    PostgresAuditLogReader,
};
pub use handlers::{
    create_mapping, create_trigger, get_audit_log, get_mapping, get_trigger, list_mappings,
    list_triggers, update_mapping, update_trigger, AdminState,
};
pub use health::healthz;
pub use normalization_mapping_repository::{
    NormalizationMappingRepository, NormalizationMappingRepositoryError,
    PostgresNormalizationMappingRepository,
};
pub use trigger_definition_repository::{
    PostgresTriggerDefinitionRepository, TriggerDefinitionRepository,
    TriggerDefinitionRepositoryError,
};
pub use trigger_publisher::{RabbitMqTriggerPublisher, TriggerPublishError, TriggerPublisher};

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: AdminState, agent_state: AgentState) -> Router {
    let admin_routes = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/trigger-definitions", post(create_trigger).get(list_triggers))
        .route("/v1/trigger-definitions/:id", get(get_trigger).put(update_trigger))
        .route("/v1/normalization-mappings", post(create_mapping).get(list_mappings))
        .route("/v1/normalization-mappings/:id", get(get_mapping).put(update_mapping))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state);

    let agent_routes = Router::new()
        .route("/v1/agents", post(create_agent).get(list_agents))
        .route("/v1/agents/by-name/:name", get(get_agent_by_name))
        .route("/v1/agents/:id", get(get_agent).put(update_agent).delete(delete_agent))
        .with_state(agent_state);

    admin_routes.merge(agent_routes)
}
