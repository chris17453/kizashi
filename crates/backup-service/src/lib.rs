//! Backup Service (ADR-0055): scheduled `pg_dump` backups of the platform's Postgres instance,
//! uploaded to an S3-compatible bucket, with every run's outcome recorded so a compliance
//! reviewer (or an operator at 3am) can answer "did last night's backup actually succeed."

mod backup_executor;
mod backup_run_repository;
mod backup_store;
mod health;
mod ops_handlers;
mod pg_dump_runner;

pub use backup_executor::{run_backup, BackupExecutorState, BackupOutcome};
pub use backup_run_repository::{
    BackupRun, BackupRunRepository, BackupRunRepositoryError, BackupRunStatus,
    PostgresBackupRunRepository,
};
pub use backup_store::{BackupStore, BackupStoreError, S3BackupStore};
pub use ops_handlers::{get_backup_status, trigger_backup};
pub use pg_dump_runner::{PgDumpError, PgDumpRunner, ProcessPgDumpRunner};

use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub run_repository: Arc<dyn BackupRunRepository>,
    pub store: Arc<dyn BackupStore>,
    pub dump_runner: Arc<dyn PgDumpRunner>,
    pub internal_secret: String,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/backup/run", post(trigger_backup))
        .route("/v1/backup/status", get(get_backup_status))
        .with_state(state)
}
