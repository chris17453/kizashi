#[cfg(test)]
#[path = "execution_repository_test.rs"]
pub(crate) mod execution_repository_test;

use async_trait::async_trait;
use common::{PipelineExecution, PipelineExecutionStatus};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ExecutionRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("pipeline execution {0} was not found")]
    NotFound(Uuid),
    #[error("pipeline execution {0} is not awaiting authoritative confirmation")]
    NotAwaitingConfirmation(Uuid),
    #[error("pipeline execution must snapshot a positive pipeline version")]
    MissingPipelineVersion,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionRequestResult {
    Created(PipelineExecution),
    Duplicate(PipelineExecution),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationResult {
    Recorded,
    Duplicate,
}

/// Durable execution coordination. An execution and its outbox event are written atomically;
/// confirmations are deduplicated by the authoritative system's event ID before reconciliation.
#[async_trait]
pub trait ExecutionRepository: Send + Sync {
    async fn create(
        &self,
        execution: PipelineExecution,
    ) -> Result<ExecutionRequestResult, ExecutionRepositoryError>;
    async fn mark_awaiting_confirmation(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<(), ExecutionRepositoryError>;
    async fn record_confirmation(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        payload: serde_json::Value,
    ) -> Result<ConfirmationResult, ExecutionRepositoryError>;
    async fn record_reconciliation_failure(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        payload: serde_json::Value,
        error: &str,
    ) -> Result<(), ExecutionRepositoryError>;
}

pub struct PostgresExecutionRepository {
    pool: sqlx::PgPool,
}

impl PostgresExecutionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

fn validate_execution(execution: &PipelineExecution) -> Result<(), ExecutionRepositoryError> {
    if execution.pipeline_version < 1 {
        return Err(ExecutionRepositoryError::MissingPipelineVersion);
    }
    Ok(())
}

type ExecutionRow = (
    Uuid,
    Uuid,
    Uuid,
    i32,
    bool,
    String,
    String,
    i32,
    serde_json::Value,
    Option<serde_json::Value>,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
);

fn status(value: PipelineExecutionStatus) -> Result<String, ExecutionRepositoryError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| {
            ExecutionRepositoryError::Backend("invalid pipeline execution status".to_string())
        })
}

fn row_to_execution(row: ExecutionRow) -> Result<PipelineExecution, ExecutionRepositoryError> {
    let (
        id,
        tenant_id,
        pipeline_definition_id,
        pipeline_version,
        command,
        idempotency_key,
        status_value,
        attempt,
        input,
        result,
        requested_at,
        started_at,
        completed_at,
    ) = row;
    Ok(PipelineExecution {
        id,
        tenant_id,
        pipeline_definition_id,
        pipeline_version,
        command,
        idempotency_key,
        status: serde_json::from_value(serde_json::json!(status_value))
            .map_err(|error| ExecutionRepositoryError::Backend(error.to_string()))?,
        attempt,
        input,
        result,
        requested_at,
        started_at,
        completed_at,
    })
}

#[async_trait]
impl ExecutionRepository for PostgresExecutionRepository {
    async fn create(
        &self,
        execution: PipelineExecution,
    ) -> Result<ExecutionRequestResult, ExecutionRepositoryError> {
        validate_execution(&execution)?;
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let inserted: Option<ExecutionRow> = sqlx::query_as(
            "INSERT INTO pipeline_executions (id,tenant_id,pipeline_definition_id,pipeline_version,command,idempotency_key,status,attempt,input,result,requested_at,started_at,completed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (tenant_id,pipeline_definition_id,idempotency_key) DO NOTHING RETURNING id,tenant_id,pipeline_definition_id,pipeline_version,command,idempotency_key,status,attempt,input,result,requested_at,started_at,completed_at"
        ).bind(execution.id).bind(execution.tenant_id).bind(execution.pipeline_definition_id).bind(execution.pipeline_version).bind(execution.command).bind(&execution.idempotency_key).bind(status(execution.status)?).bind(execution.attempt).bind(&execution.input).bind(&execution.result).bind(execution.requested_at).bind(execution.started_at).bind(execution.completed_at).fetch_optional(&mut *tx).await.map_err(backend)?;
        if let Some(row) = inserted {
            let execution = row_to_execution(row)?;
            write_outbox(
                &mut tx,
                execution.tenant_id,
                execution.id,
                "pipeline.execution.queued",
                serde_json::to_value(&execution).unwrap_or_default(),
            )
            .await?;
            tx.commit().await.map_err(backend)?;
            return Ok(ExecutionRequestResult::Created(execution));
        }
        let existing: ExecutionRow = sqlx::query_as("SELECT id,tenant_id,pipeline_definition_id,pipeline_version,command,idempotency_key,status,attempt,input,result,requested_at,started_at,completed_at FROM pipeline_executions WHERE tenant_id=$1 AND pipeline_definition_id=$2 AND idempotency_key=$3")
            .bind(execution.tenant_id).bind(execution.pipeline_definition_id).bind(&execution.idempotency_key).fetch_one(&mut *tx).await.map_err(backend)?;
        tx.rollback().await.map_err(backend)?;
        Ok(ExecutionRequestResult::Duplicate(row_to_execution(existing)?))
    }

    async fn mark_awaiting_confirmation(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
    ) -> Result<(), ExecutionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let updated: Option<ExecutionRow> = sqlx::query_as("UPDATE pipeline_executions SET status=$1, completed_at=NULL WHERE tenant_id=$2 AND id=$3 AND command=true AND status IN ('queued','running') RETURNING id,tenant_id,pipeline_definition_id,pipeline_version,command,idempotency_key,status,attempt,input,result,requested_at,started_at,completed_at")
            .bind(status(PipelineExecutionStatus::AwaitingConfirmation)?).bind(tenant_id).bind(execution_id).fetch_optional(&mut *tx).await.map_err(backend)?;
        let execution =
            updated.ok_or(ExecutionRepositoryError::NotAwaitingConfirmation(execution_id))?;
        let execution = row_to_execution(execution)?;
        write_outbox(
            &mut tx,
            tenant_id,
            execution_id,
            "pipeline.command.dispatched",
            serde_json::to_value(execution).unwrap_or_default(),
        )
        .await?;
        tx.commit().await.map_err(backend)
    }

    async fn record_confirmation(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        payload: serde_json::Value,
    ) -> Result<ConfirmationResult, ExecutionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let inserted: Option<(Uuid,)> = sqlx::query_as("INSERT INTO pipeline_confirmation_inbox (id,tenant_id,execution_id,source_event_id,payload,received_at) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (tenant_id,source_event_id) DO NOTHING RETURNING id")
            .bind(Uuid::new_v4()).bind(tenant_id).bind(execution_id).bind(source_event_id).bind(&payload).bind(chrono::Utc::now()).fetch_optional(&mut *tx).await.map_err(backend)?;
        if inserted.is_none() {
            tx.rollback().await.map_err(backend)?;
            return Ok(ConfirmationResult::Duplicate);
        }
        let affected = sqlx::query("UPDATE pipeline_executions SET status=$1,result=$2,completed_at=$3 WHERE tenant_id=$4 AND id=$5 AND status=$6")
            .bind(status(PipelineExecutionStatus::Confirmed)?).bind(&payload).bind(chrono::Utc::now()).bind(tenant_id).bind(execution_id).bind(status(PipelineExecutionStatus::AwaitingConfirmation)?).execute(&mut *tx).await.map_err(backend)?.rows_affected();
        if affected != 1 {
            return Err(ExecutionRepositoryError::NotAwaitingConfirmation(execution_id));
        }
        write_outbox(&mut tx, tenant_id, execution_id, "pipeline.command.confirmed", payload)
            .await?;
        tx.commit().await.map_err(backend)?;
        Ok(ConfirmationResult::Recorded)
    }
    async fn record_reconciliation_failure(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        payload: serde_json::Value,
        error: &str,
    ) -> Result<(), ExecutionRepositoryError> {
        sqlx::query("INSERT INTO pipeline_reconciliation_failures (id,tenant_id,execution_id,source_event_id,error,payload) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (tenant_id,execution_id,source_event_id) DO UPDATE SET error=EXCLUDED.error,payload=EXCLUDED.payload,created_at=now(),resolved_at=NULL")
            .bind(Uuid::new_v4()).bind(tenant_id).bind(execution_id).bind(source_event_id).bind(error).bind(payload).execute(&self.pool).await.map_err(backend)?;
        Ok(())
    }
}

fn backend(error: sqlx::Error) -> ExecutionRepositoryError {
    ExecutionRepositoryError::Backend(error.to_string())
}

async fn write_outbox(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    execution_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), ExecutionRepositoryError> {
    sqlx::query("INSERT INTO pipeline_outbox (id,tenant_id,execution_id,event_type,payload,created_at) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(tenant_id).bind(execution_id).bind(event_type).bind(payload).bind(chrono::Utc::now()).execute(&mut **tx).await.map_err(backend)?;
    Ok(())
}
