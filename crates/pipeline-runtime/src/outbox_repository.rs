#[cfg(test)]
#[path = "outbox_repository_test.rs"]
mod outbox_repository_test;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxMessage {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub execution_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum OutboxRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    async fn lease(
        &self,
        limit: i64,
        lease_for: Duration,
    ) -> Result<Vec<OutboxMessage>, OutboxRepositoryError>;
    async fn mark_published(&self, id: Uuid) -> Result<(), OutboxRepositoryError>;
    async fn release(&self, id: Uuid, error: &str) -> Result<(), OutboxRepositoryError>;
}

pub struct PostgresOutboxRepository {
    pool: sqlx::PgPool,
}
impl PostgresOutboxRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}
type Row = (Uuid, Uuid, Uuid, String, serde_json::Value);
fn value(row: Row) -> OutboxMessage {
    let (id, tenant_id, execution_id, event_type, payload) = row;
    OutboxMessage { id, tenant_id, execution_id, event_type, payload }
}
fn backend(error: sqlx::Error) -> OutboxRepositoryError {
    OutboxRepositoryError::Backend(error.to_string())
}

#[async_trait]
impl OutboxRepository for PostgresOutboxRepository {
    async fn lease(
        &self,
        limit: i64,
        lease_for: Duration,
    ) -> Result<Vec<OutboxMessage>, OutboxRepositoryError> {
        let until = Utc::now() + lease_for;
        let rows: Vec<Row> = sqlx::query_as("WITH candidates AS (SELECT id FROM pipeline_outbox WHERE published_at IS NULL AND dead_lettered_at IS NULL AND (lease_expires_at IS NULL OR lease_expires_at < now()) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE pipeline_outbox o SET lease_expires_at=$2,publish_attempts=publish_attempts+1 FROM candidates WHERE o.id=candidates.id RETURNING o.id,o.tenant_id,o.execution_id,o.event_type,o.payload")
            .bind(limit).bind(until).fetch_all(&self.pool).await.map_err(backend)?;
        Ok(rows.into_iter().map(value).collect())
    }
    async fn mark_published(&self, id: Uuid) -> Result<(), OutboxRepositoryError> {
        sqlx::query("UPDATE pipeline_outbox SET published_at=now(),lease_expires_at=NULL,last_error=NULL WHERE id=$1") .bind(id).execute(&self.pool).await.map_err(backend)?;
        Ok(())
    }
    async fn release(&self, id: Uuid, error: &str) -> Result<(), OutboxRepositoryError> {
        sqlx::query("UPDATE pipeline_outbox SET lease_expires_at=NULL,last_error=$2,dead_lettered_at=CASE WHEN publish_attempts >= 8 THEN now() ELSE dead_lettered_at END WHERE id=$1 AND published_at IS NULL").bind(id).bind(error).execute(&self.pool).await.map_err(backend)?;
        Ok(())
    }
}
