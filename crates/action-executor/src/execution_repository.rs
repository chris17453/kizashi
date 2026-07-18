#[path = "execution_repository_test.rs"]
#[cfg(test)]
pub(crate) mod execution_repository_test;

use async_trait::async_trait;
use common::ActionExecution;
use thiserror::Error;

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
}

pub struct PostgresExecutionRepository {
    pool: sqlx::PgPool,
}

impl PostgresExecutionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionRepository for PostgresExecutionRepository {
    async fn insert(&self, execution: &ActionExecution) -> Result<(), ExecutionRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO action_executions
                (id, trigger_id, event_id, action_type, status, executed_at, detail)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(execution.id)
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
}
