#[path = "event_type_definition_repository_test.rs"]
#[cfg(test)]
pub(crate) mod event_type_definition_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::EventTypeDefinition;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EventTypeDefinitionRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no event type definition with id {0}")]
    NotFound(Uuid),
}

#[async_trait]
pub trait EventTypeDefinitionRepository: Send + Sync {
    async fn create(
        &self,
        definition: EventTypeDefinition,
        actor: &str,
    ) -> Result<EventTypeDefinition, EventTypeDefinitionRepositoryError>;
    async fn create_version(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        field_schema: serde_json::Value,
        actor: &str,
    ) -> Result<EventTypeDefinition, EventTypeDefinitionRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<EventTypeDefinition>, EventTypeDefinitionRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        all_versions: bool,
    ) -> Result<Vec<EventTypeDefinition>, EventTypeDefinitionRepositoryError>;
}

pub struct PostgresEventTypeDefinitionRepository {
    pool: sqlx::PgPool,
}

impl PostgresEventTypeDefinitionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type DefinitionRow = (Uuid, Uuid, String, serde_json::Value, i32);

fn row_to_definition(row: DefinitionRow) -> EventTypeDefinition {
    let (id, tenant_id, name, field_schema, version) = row;
    EventTypeDefinition { id, tenant_id, name, field_schema, version }
}

#[async_trait]
impl EventTypeDefinitionRepository for PostgresEventTypeDefinitionRepository {
    async fn create(
        &self,
        definition: EventTypeDefinition,
        actor: &str,
    ) -> Result<EventTypeDefinition, EventTypeDefinitionRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        sqlx::query("INSERT INTO event_type_definitions (id, tenant_id, name, field_schema, version) VALUES ($1, $2, $3, $4, $5)")
            .bind(definition.id).bind(definition.tenant_id).bind(&definition.name).bind(&definition.field_schema).bind(definition.version)
            .execute(&mut *tx).await.map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: definition.tenant_id,
                entity_type: "event_type_definition".into(),
                entity_id: definition.id,
                change_type: ChangeType::Created,
                actor: actor.into(),
                before: None,
                after: serde_json::to_value(&definition).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(definition)
    }

    async fn create_version(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        field_schema: serde_json::Value,
        actor: &str,
    ) -> Result<EventTypeDefinition, EventTypeDefinitionRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        let existing: Option<DefinitionRow> = sqlx::query_as("SELECT id, tenant_id, name, field_schema, version FROM event_type_definitions WHERE id = $1 AND tenant_id = $2")
            .bind(id).bind(tenant_id).fetch_optional(&mut *tx).await.map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        let Some(existing) = existing else {
            return Err(EventTypeDefinitionRepositoryError::NotFound(id));
        };
        let before = row_to_definition(existing);
        let next = EventTypeDefinition {
            id: Uuid::new_v4(),
            tenant_id,
            name: before.name.clone(),
            field_schema,
            version: before.version + 1,
        };
        sqlx::query("INSERT INTO event_type_definitions (id, tenant_id, name, field_schema, version) VALUES ($1, $2, $3, $4, $5)")
            .bind(next.id).bind(next.tenant_id).bind(&next.name).bind(&next.field_schema).bind(next.version)
            .execute(&mut *tx).await.map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "event_type_definition".into(),
                entity_id: next.id,
                change_type: ChangeType::Created,
                actor: actor.into(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&next).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(next)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<EventTypeDefinition>, EventTypeDefinitionRepositoryError> {
        let row: Option<DefinitionRow> = sqlx::query_as("SELECT id, tenant_id, name, field_schema, version FROM event_type_definitions WHERE id = $1 AND tenant_id = $2")
            .bind(id).bind(tenant_id).fetch_optional(&self.pool).await.map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_definition))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        all_versions: bool,
    ) -> Result<Vec<EventTypeDefinition>, EventTypeDefinitionRepositoryError> {
        let sql = if all_versions {
            "SELECT id, tenant_id, name, field_schema, version FROM event_type_definitions WHERE tenant_id = $1 ORDER BY name, version DESC"
        } else {
            "SELECT DISTINCT ON (name) id, tenant_id, name, field_schema, version FROM event_type_definitions WHERE tenant_id = $1 ORDER BY name, version DESC"
        };
        let rows: Vec<DefinitionRow> = sqlx::query_as(sql)
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| EventTypeDefinitionRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_definition).collect())
    }
}
