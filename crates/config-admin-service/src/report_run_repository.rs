#[path = "report_run_repository_test.rs"]
#[cfg(test)]
pub(crate) mod report_run_repository_test;

use async_trait::async_trait;
use common::ReportRun;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ReportRunRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no report run with id {0}")]
    NotFound(Uuid),
}

#[async_trait]
pub trait ReportRunRepository: Send + Sync {
    async fn create(&self, run: ReportRun) -> Result<ReportRun, ReportRunRepositoryError>;
    async fn update(&self, run: ReportRun) -> Result<ReportRun, ReportRunRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        schedule_id: Option<Uuid>,
    ) -> Result<Vec<ReportRun>, ReportRunRepositoryError>;
}

pub struct PostgresReportRunRepository {
    pool: sqlx::PgPool,
}
impl PostgresReportRunRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type RunRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
);
fn row_to_run(row: RunRow) -> ReportRun {
    let (
        id,
        tenant_id,
        schedule_id,
        schedule_name,
        recipient,
        format,
        status,
        error,
        artifact_url,
        started_at,
        completed_at,
    ) = row;
    ReportRun {
        id,
        tenant_id,
        schedule_id,
        schedule_name,
        recipient,
        format,
        status,
        error,
        artifact_url,
        started_at,
        completed_at,
    }
}

#[async_trait]
impl ReportRunRepository for PostgresReportRunRepository {
    async fn create(&self, run: ReportRun) -> Result<ReportRun, ReportRunRepositoryError> {
        sqlx::query("INSERT INTO report_runs (id, tenant_id, schedule_id, schedule_name, recipient, format, status, error, artifact_url, started_at, completed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(run.id).bind(run.tenant_id).bind(run.schedule_id).bind(&run.schedule_name).bind(&run.recipient).bind(&run.format).bind(&run.status).bind(&run.error).bind(&run.artifact_url).bind(run.started_at).bind(run.completed_at)
            .execute(&self.pool).await.map_err(|e| ReportRunRepositoryError::Backend(e.to_string()))?;
        Ok(run)
    }

    async fn update(&self, run: ReportRun) -> Result<ReportRun, ReportRunRepositoryError> {
        let result = sqlx::query("UPDATE report_runs SET status=$1, error=$2, artifact_url=$3, completed_at=$4 WHERE id=$5 AND tenant_id=$6")
            .bind(&run.status).bind(&run.error).bind(&run.artifact_url).bind(run.completed_at).bind(run.id).bind(run.tenant_id)
            .execute(&self.pool).await.map_err(|e| ReportRunRepositoryError::Backend(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(ReportRunRepositoryError::NotFound(run.id));
        }
        Ok(run)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        schedule_id: Option<Uuid>,
    ) -> Result<Vec<ReportRun>, ReportRunRepositoryError> {
        let rows: Vec<RunRow> = if let Some(schedule_id) = schedule_id {
            sqlx::query_as("SELECT id,tenant_id,schedule_id,schedule_name,recipient,format,status,error,artifact_url,started_at,completed_at FROM report_runs WHERE tenant_id=$1 AND schedule_id=$2 ORDER BY started_at DESC LIMIT 100").bind(tenant_id).bind(schedule_id).fetch_all(&self.pool).await
        } else {
            sqlx::query_as("SELECT id,tenant_id,schedule_id,schedule_name,recipient,format,status,error,artifact_url,started_at,completed_at FROM report_runs WHERE tenant_id=$1 ORDER BY started_at DESC LIMIT 100").bind(tenant_id).fetch_all(&self.pool).await
        }.map_err(|e| ReportRunRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_run).collect())
    }
}
