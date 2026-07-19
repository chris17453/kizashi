//! Retention/Archival Service (spec §6, service #12): enforces retention policy by archiving
//! aged `RawRecord` rows to S3-compatible object storage (ADR-0005 format) and hard-deleting
//! them from the hot store, and supports reimport of archived batches (spec §9). See ADR-0011
//! for this service's v1 scope: self-owned retention policy store, S3-compatible archival
//! backend tested against MinIO, and why reimport bypasses Ingestion Gateway.

mod archive_store;
mod audit_log;
mod health;
mod manifest;
mod ops_handlers;
mod policy_handlers;
mod raw_record_client;
mod reimport;
mod retention_policy;
mod sweep;

pub use archive_store::{ArchiveStore, ArchiveStoreError, S3ArchiveStore};
pub use audit_log::{
    record_audit_entry, AuditLogEntry, AuditLogError, AuditLogReader, ChangeType,
    PostgresAuditLogReader,
};
pub use health::healthz;
pub use manifest::{archive_key, ArchiveManifest};
pub use ops_handlers::{trigger_reimport, trigger_sweep, ReimportRequest};
pub use policy_handlers::{
    create_policy, delete_policy, get_audit_log, get_policy, list_policies, update_policy,
};
pub use raw_record_client::{HttpRawRecordClient, RawRecordClient, RawRecordClientError};
pub use reimport::{reimport, ReimportError, ReimportState, ReimportSummary};
pub use retention_policy::{
    DataClass, PostgresRetentionPolicyRepository, RetentionPolicy, RetentionPolicyRepository,
    RetentionPolicyRepositoryError,
};
pub use sweep::{sweep, SweepError, SweepState, SweepSummary};

use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub policy_repository: Arc<dyn RetentionPolicyRepository>,
    pub audit_reader: Arc<dyn AuditLogReader>,
    pub record_client: Arc<dyn RawRecordClient>,
    pub archive_store: Arc<dyn ArchiveStore>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/retention-policies", post(create_policy).get(list_policies))
        .route(
            "/v1/retention-policies/:id",
            get(get_policy).put(update_policy).delete(delete_policy),
        )
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .route("/v1/sweep", post(trigger_sweep))
        .route("/v1/reimport", post(trigger_reimport))
        .with_state(state)
}
