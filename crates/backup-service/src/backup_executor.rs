#[path = "backup_executor_test.rs"]
#[cfg(test)]
mod backup_executor_test;

use crate::backup_run_repository::{BackupRunRepository, BackupRunStatus};
use crate::backup_store::BackupStore;
use crate::pg_dump_runner::PgDumpRunner;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct BackupExecutorState {
    pub run_repository: Arc<dyn BackupRunRepository>,
    pub store: Arc<dyn BackupStore>,
    pub dump_runner: Arc<dyn PgDumpRunner>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct BackupOutcome {
    pub id: Uuid,
    pub status: BackupRunStatus,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
}

/// Runs one full backup pass (ADR-0055): dump Postgres via `pg_dump`, upload the archive, record
/// the outcome. Every failure branch still tries to write a `Failed` row so the status page
/// reflects reality instead of the run silently vanishing -- a compliance reviewer asking "did
/// last night's backup succeed" needs a "no" as reliably as a "yes".
pub async fn run_backup(state: &BackupExecutorState, now: DateTime<Utc>) -> BackupOutcome {
    let id = Uuid::new_v4();
    let target = format!("postgres/{}.dump", now.format("%Y-%m-%dT%H-%M-%SZ"));

    if let Err(e) = state.run_repository.start(id, now, &target).await {
        return BackupOutcome {
            id,
            status: BackupRunStatus::Failed,
            size_bytes: None,
            error: Some(format!("failed to record backup start: {e}")),
        };
    }

    let bytes = match state.dump_runner.dump().await {
        Ok(bytes) => bytes,
        Err(e) => {
            let _ = state.run_repository.fail(id, now, &e.to_string()).await;
            return BackupOutcome {
                id,
                status: BackupRunStatus::Failed,
                size_bytes: None,
                error: Some(e.to_string()),
            };
        }
    };
    let size_bytes = bytes.len() as i64;

    if let Err(e) = state.store.upload(&target, bytes).await {
        let _ = state.run_repository.fail(id, now, &e.to_string()).await;
        return BackupOutcome {
            id,
            status: BackupRunStatus::Failed,
            size_bytes: None,
            error: Some(e.to_string()),
        };
    }

    if let Err(e) = state.run_repository.complete(id, now, size_bytes).await {
        return BackupOutcome {
            id,
            status: BackupRunStatus::Failed,
            size_bytes: Some(size_bytes),
            error: Some(format!("backup succeeded but failed to record completion: {e}")),
        };
    }

    BackupOutcome {
        id,
        status: BackupRunStatus::Success,
        size_bytes: Some(size_bytes),
        error: None,
    }
}
