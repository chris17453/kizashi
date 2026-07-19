#[path = "agent_repository_test.rs"]
#[cfg(test)]
pub(crate) mod agent_repository_test;

use async_trait::async_trait;
use common::Agent;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// One synced Agent plus scheduling bookkeeping — `last_polled_at` lives here, not on
/// `common::Agent`, since it's purely a scheduling concern local to this service, not part of
/// the Agent registry's own schema.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredAgent {
    pub agent: Agent,
    pub last_polled_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Agent Scheduler's own copy of the Agent registry (ADR-0020), kept current by consuming
/// `agent.changed` bus messages — never written to except by that consumer.
#[async_trait]
pub trait AgentRepository: Send + Sync {
    async fn upsert(&self, agent: Agent) -> Result<(), AgentRepositoryError>;
    async fn delete(&self, id: Uuid) -> Result<(), AgentRepositoryError>;
    async fn list_enabled(&self) -> Result<Vec<StoredAgent>, AgentRepositoryError>;
    async fn mark_polled(
        &self,
        id: Uuid,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AgentRepositoryError>;
}

pub struct PostgresAgentRepository {
    pool: sqlx::PgPool,
}

impl PostgresAgentRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRepository for PostgresAgentRepository {
    async fn upsert(&self, agent: Agent) -> Result<(), AgentRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO agents (id, tenant_id, connector_type, name, config, enabled)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                connector_type = EXCLUDED.connector_type,
                name = EXCLUDED.name,
                config = EXCLUDED.config,
                enabled = EXCLUDED.enabled
            "#,
        )
        .bind(agent.id)
        .bind(agent.tenant_id)
        .bind(&agent.connector_type)
        .bind(&agent.name)
        .bind(sqlx::types::Json(&agent.config))
        .bind(agent.enabled)
        .execute(&self.pool)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), AgentRepositoryError> {
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_enabled(&self) -> Result<Vec<StoredAgent>, AgentRepositoryError> {
        type Row = (
            Uuid,
            Uuid,
            String,
            String,
            sqlx::types::Json<serde_json::Value>,
            bool,
            Option<chrono::DateTime<chrono::Utc>>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, connector_type, name, config, enabled, last_polled_at
            FROM agents
            WHERE enabled = true
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(id, tenant_id, connector_type, name, config, enabled, last_polled_at)| {
                StoredAgent {
                    agent: Agent { id, tenant_id, connector_type, name, config: config.0, enabled },
                    last_polled_at,
                }
            })
            .collect())
    }

    async fn mark_polled(
        &self,
        id: Uuid,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AgentRepositoryError> {
        sqlx::query("UPDATE agents SET last_polled_at = $1 WHERE id = $2")
            .bind(at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
