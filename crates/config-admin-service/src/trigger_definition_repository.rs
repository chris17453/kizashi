#[path = "trigger_definition_repository_test.rs"]
#[cfg(test)]
pub(crate) mod trigger_definition_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::TriggerDefinition;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TriggerDefinitionRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no trigger definition with id {0}")]
    NotFound(Uuid),
}

/// CRUD for TriggerDefinition, config-admin's own Postgres schema (ADR-0010). Every
/// create/update writes one audit_log row in the same transaction as the entity change.
#[async_trait]
pub trait TriggerDefinitionRepository: Send + Sync {
    async fn create(
        &self,
        trigger: TriggerDefinition,
        actor: &str,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError>;
    async fn update(
        &self,
        trigger: TriggerDefinition,
        actor: &str,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerDefinitionRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TriggerDefinition>, TriggerDefinitionRepositoryError>;
}

pub struct PostgresTriggerDefinitionRepository {
    pool: sqlx::PgPool,
}

impl PostgresTriggerDefinitionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type TriggerRow = (
    Uuid,
    Uuid,
    String,
    String,
    sqlx::types::Json<common::TriggerCondition>,
    i64,
    sqlx::types::Json<Vec<common::ActionRef>>,
    bool,
);

fn row_to_trigger(row: TriggerRow) -> TriggerDefinition {
    let (id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled) = row;
    TriggerDefinition {
        id,
        tenant_id,
        name,
        event_type_match,
        condition: condition.0,
        window_seconds,
        actions: actions.0,
        enabled,
    }
}

#[async_trait]
impl TriggerDefinitionRepository for PostgresTriggerDefinitionRepository {
    async fn create(
        &self,
        trigger: TriggerDefinition,
        actor: &str,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO trigger_definitions
                (id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(trigger.id)
        .bind(trigger.tenant_id)
        .bind(&trigger.name)
        .bind(&trigger.event_type_match)
        .bind(sqlx::types::Json(&trigger.condition))
        .bind(trigger.window_seconds)
        .bind(sqlx::types::Json(&trigger.actions))
        .bind(trigger.enabled)
        .execute(&mut *tx)
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: trigger.tenant_id,
                entity_type: "trigger_definition".to_string(),
                entity_id: trigger.id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::to_value(&trigger).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(trigger)
    }

    async fn update(
        &self,
        trigger: TriggerDefinition,
        actor: &str,
    ) -> Result<TriggerDefinition, TriggerDefinitionRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        let existing: Option<TriggerRow> = sqlx::query_as(
            "SELECT id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled FROM trigger_definitions WHERE id = $1 AND tenant_id = $2",
        )
        .bind(trigger.id)
        .bind(trigger.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(TriggerDefinitionRepositoryError::NotFound(trigger.id));
        };
        let before = row_to_trigger(existing);

        sqlx::query(
            r#"
            UPDATE trigger_definitions
            SET name = $1, event_type_match = $2, condition = $3, window_seconds = $4, actions = $5, enabled = $6
            WHERE id = $7 AND tenant_id = $8
            "#,
        )
        .bind(&trigger.name)
        .bind(&trigger.event_type_match)
        .bind(sqlx::types::Json(&trigger.condition))
        .bind(trigger.window_seconds)
        .bind(sqlx::types::Json(&trigger.actions))
        .bind(trigger.enabled)
        .bind(trigger.id)
        .bind(trigger.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: trigger.tenant_id,
                entity_type: "trigger_definition".to_string(),
                entity_id: trigger.id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&trigger).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(trigger)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerDefinitionRepositoryError> {
        let row: Option<TriggerRow> = sqlx::query_as(
            "SELECT id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled FROM trigger_definitions WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_trigger))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TriggerDefinition>, TriggerDefinitionRepositoryError> {
        let rows: Vec<TriggerRow> = sqlx::query_as(
            "SELECT id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled FROM trigger_definitions WHERE tenant_id = $1 ORDER BY name LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| TriggerDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_trigger).collect())
    }
}
