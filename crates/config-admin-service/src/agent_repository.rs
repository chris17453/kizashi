#[path = "agent_repository_test.rs"]
#[cfg(test)]
pub(crate) mod agent_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::Agent;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no agent with id {0}")]
    NotFound(Uuid),
}

/// CRUD for Agent, config-admin's own Postgres schema — same audit-logging convention as
/// TriggerDefinition/NormalizationMapping (CLAUDE.md §5): every create/update/delete writes one
/// audit_log row in the same transaction as the entity change.
#[async_trait]
pub trait AgentRepository: Send + Sync {
    async fn create(&self, agent: Agent) -> Result<Agent, AgentRepositoryError>;
    async fn update(&self, agent: Agent) -> Result<Agent, AgentRepositoryError>;
    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Agent>, AgentRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Agent>, AgentRepositoryError>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AgentRepositoryError>;

    /// Looks up an agent by its registered `name` — the join key Ingestion Gateway uses to
    /// enforce enabled/disabled status at ingest time (an agent's `name` is what a deployed
    /// connector's own `CONNECTOR_ID` is set to, per `AgentsClient`'s documented convention).
    async fn find_by_name(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<Agent>, AgentRepositoryError>;
}

pub struct PostgresAgentRepository {
    pool: sqlx::PgPool,
}

impl PostgresAgentRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type AgentRow = (Uuid, Uuid, String, String, serde_json::Value, bool);

fn row_to_agent(row: AgentRow) -> Agent {
    let (id, tenant_id, connector_type, name, config, enabled) = row;
    Agent { id, tenant_id, connector_type, name, config, enabled }
}

#[async_trait]
impl AgentRepository for PostgresAgentRepository {
    async fn create(&self, agent: Agent) -> Result<Agent, AgentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            "INSERT INTO agents (id, tenant_id, connector_type, name, config, enabled) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(agent.id)
        .bind(agent.tenant_id)
        .bind(&agent.connector_type)
        .bind(&agent.name)
        .bind(&agent.config)
        .bind(agent.enabled)
        .execute(&mut *tx)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: agent.tenant_id,
                entity_type: "agent".to_string(),
                entity_id: agent.id,
                change_type: ChangeType::Created,
                actor: agent.tenant_id.to_string(),
                before: None,
                after: serde_json::to_value(&agent).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(agent)
    }

    async fn update(&self, agent: Agent) -> Result<Agent, AgentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        let existing: Option<AgentRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(agent.id)
        .bind(agent.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(AgentRepositoryError::NotFound(agent.id));
        };
        let before = row_to_agent(existing);

        sqlx::query(
            "UPDATE agents SET connector_type = $1, name = $2, config = $3, enabled = $4 WHERE id = $5 AND tenant_id = $6",
        )
        .bind(&agent.connector_type)
        .bind(&agent.name)
        .bind(&agent.config)
        .bind(agent.enabled)
        .bind(agent.id)
        .bind(agent.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: agent.tenant_id,
                entity_type: "agent".to_string(),
                entity_id: agent.id,
                change_type: ChangeType::Updated,
                actor: agent.tenant_id.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&agent).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(agent)
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Agent>, AgentRepositoryError> {
        let row: Option<AgentRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_agent))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Agent>, AgentRepositoryError> {
        let rows: Vec<AgentRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE tenant_id = $1 ORDER BY name LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_agent).collect())
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AgentRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        let existing: Option<AgentRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(AgentRepositoryError::NotFound(id));
        };
        let before = row_to_agent(existing);

        sqlx::query("DELETE FROM agents WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "agent".to_string(),
                entity_id: id,
                change_type: ChangeType::Deleted,
                actor: tenant_id.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::Value::Null,
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn find_by_name(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<Agent>, AgentRepositoryError> {
        let row: Option<AgentRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE tenant_id = $1 AND name = $2",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_agent))
    }
}
