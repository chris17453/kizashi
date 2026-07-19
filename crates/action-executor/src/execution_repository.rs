#[path = "execution_repository_test.rs"]
#[cfg(test)]
pub(crate) mod execution_repository_test;

use async_trait::async_trait;
use common::ActionExecution;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ExecutionRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Append-only audit log of action executions (spec §5.5, CLAUDE.md §5: "Action executions are
/// append-only — never update-in-place"). No update or delete method exists on this trait —
/// a retry is a new `ActionExecution::retry(...)` row, per the type's own design in `common`.
#[async_trait]
pub trait ExecutionRepository: Send + Sync {
    async fn insert(&self, execution: &ActionExecution) -> Result<(), ExecutionRepositoryError>;

    /// Lists every execution (including retries) for one Event, tenant-scoped — the
    /// event→action hop of the platform's full data lineage (ADR-0017), and the read path a
    /// record-journey view uses to show what actually happened after an Event fired.
    async fn list_by_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecution>, ExecutionRepositoryError>;
}

pub struct PostgresExecutionRepository {
    pool: sqlx::PgPool,
}

impl PostgresExecutionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type ExecutionRow = (
    Uuid,
    Uuid,
    Uuid,
    Uuid,
    sqlx::types::Json<common::ActionType>,
    sqlx::types::Json<common::ActionExecutionStatus>,
    chrono::DateTime<chrono::Utc>,
    serde_json::Value,
);

fn row_to_execution(row: ExecutionRow) -> ActionExecution {
    let (id, tenant_id, trigger_id, event_id, action_type, status, executed_at, detail) = row;
    ActionExecution {
        id,
        tenant_id,
        trigger_id,
        event_id,
        action_type: action_type.0,
        status: status.0,
        executed_at,
        detail,
    }
}

#[async_trait]
impl ExecutionRepository for PostgresExecutionRepository {
    async fn insert(&self, execution: &ActionExecution) -> Result<(), ExecutionRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO action_executions
                (id, tenant_id, trigger_id, event_id, action_type, status, executed_at, detail)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(execution.id)
        .bind(execution.tenant_id)
        .bind(execution.trigger_id)
        .bind(execution.event_id)
        .bind(sqlx::types::Json(execution.action_type))
        .bind(sqlx::types::Json(execution.status))
        .bind(execution.executed_at)
        .bind(&execution.detail)
        .execute(&self.pool)
        .await
        .map_err(|e| ExecutionRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_by_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecution>, ExecutionRepositoryError> {
        let rows: Vec<ExecutionRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, trigger_id, event_id, action_type, status, executed_at, detail
            FROM action_executions
            WHERE tenant_id = $1 AND event_id = $2
            ORDER BY executed_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(event_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ExecutionRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_execution).collect())
    }
}
