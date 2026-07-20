#[path = "backup_run_repository_test.rs"]
#[cfg(test)]
pub(crate) mod backup_run_repository_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BackupRunRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupRunStatus {
    Running,
    Success,
    Failed,
}

impl BackupRunStatus {
    fn as_str(&self) -> &'static str {
        match self {
            BackupRunStatus::Running => "running",
            BackupRunStatus::Success => "success",
            BackupRunStatus::Failed => "failed",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "success" => BackupRunStatus::Success,
            "failed" => BackupRunStatus::Failed,
            _ => BackupRunStatus::Running,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BackupRun {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: BackupRunStatus,
    pub target: String,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
}

/// Records every backup attempt (ADR-0055), success or failure — the "when did we last actually
/// back this platform up, and did it work" record a compliance reviewer asks for. Unlike
/// `auth_audit_log`/`login_attempts`, this is operational status, not an audit trail: a row is
/// created `Running` and transitions exactly once to `Success` or `Failed` as the backup
/// completes, rather than being append-only-immutable.
#[async_trait]
pub trait BackupRunRepository: Send + Sync {
    async fn start(
        &self,
        id: Uuid,
        started_at: DateTime<Utc>,
        target: &str,
    ) -> Result<(), BackupRunRepositoryError>;

    async fn complete(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        size_bytes: i64,
    ) -> Result<(), BackupRunRepositoryError>;

    async fn fail(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        error: &str,
    ) -> Result<(), BackupRunRepositoryError>;

    /// `before`, when present, only returns runs started strictly earlier than that timestamp
    /// -- the same exclusive keyset-cursor pagination shape `/audit-log`'s "Load older" link and
    /// `login_attempts` already use (ADR-0063), so a tenant's backup history isn't capped at
    /// the first `limit` rows forever as backups accumulate.
    async fn list_recent(
        &self,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<BackupRun>, BackupRunRepositoryError>;
}

pub struct PostgresBackupRunRepository {
    pool: sqlx::PgPool,
}

impl PostgresBackupRunRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type BackupRunRow =
    (Uuid, DateTime<Utc>, Option<DateTime<Utc>>, String, String, Option<i64>, Option<String>);

fn row_to_run(row: BackupRunRow) -> BackupRun {
    let (id, started_at, completed_at, status, target, size_bytes, error) = row;
    BackupRun {
        id,
        started_at,
        completed_at,
        status: BackupRunStatus::parse(&status),
        target,
        size_bytes,
        error,
    }
}

#[async_trait]
impl BackupRunRepository for PostgresBackupRunRepository {
    async fn start(
        &self,
        id: Uuid,
        started_at: DateTime<Utc>,
        target: &str,
    ) -> Result<(), BackupRunRepositoryError> {
        sqlx::query(
            "INSERT INTO backup_runs (id, started_at, status, target) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(started_at)
        .bind(BackupRunStatus::Running.as_str())
        .bind(target)
        .execute(&self.pool)
        .await
        .map_err(|e| BackupRunRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn complete(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        size_bytes: i64,
    ) -> Result<(), BackupRunRepositoryError> {
        sqlx::query(
            "UPDATE backup_runs SET status = $1, completed_at = $2, size_bytes = $3 WHERE id = $4",
        )
        .bind(BackupRunStatus::Success.as_str())
        .bind(completed_at)
        .bind(size_bytes)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| BackupRunRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn fail(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        error: &str,
    ) -> Result<(), BackupRunRepositoryError> {
        sqlx::query(
            "UPDATE backup_runs SET status = $1, completed_at = $2, error = $3 WHERE id = $4",
        )
        .bind(BackupRunStatus::Failed.as_str())
        .bind(completed_at)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| BackupRunRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_recent(
        &self,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<BackupRun>, BackupRunRepositoryError> {
        let rows: Vec<BackupRunRow> = match before {
            Some(before) => sqlx::query_as(
                "SELECT id, started_at, completed_at, status, target, size_bytes, error FROM backup_runs WHERE started_at < $1 ORDER BY started_at DESC LIMIT $2",
            )
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await,
            None => sqlx::query_as(
                "SELECT id, started_at, completed_at, status, target, size_bytes, error FROM backup_runs ORDER BY started_at DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await,
        }
        .map_err(|e| BackupRunRepositoryError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_run).collect())
    }
}
