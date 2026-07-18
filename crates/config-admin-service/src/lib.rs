//! Config/Admin Service (spec §6, service #11): CRUD + immutable audit logging for
//! TriggerDefinition and NormalizationMapping in v1 (ADR-0010). Every admin/config mutation
//! writes an audit_log row in the same transaction as the entity change (CLAUDE.md §5).

mod audit_log;
mod handlers;
mod health;
mod normalization_mapping_repository;
mod trigger_definition_repository;

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

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: AdminState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/trigger-definitions", post(create_trigger).get(list_triggers))
        .route("/v1/trigger-definitions/:id", get(get_trigger).put(update_trigger))
        .route("/v1/normalization-mappings", post(create_mapping).get(list_mappings))
        .route("/v1/normalization-mappings/:id", get(get_mapping).put(update_mapping))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state)
}
