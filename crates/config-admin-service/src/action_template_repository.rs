#[path = "action_template_repository_test.rs"]
#[cfg(test)]
pub(crate) mod action_template_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::{ActionTemplate, ActionType};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ActionTemplateRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no action template with id {0}")]
    NotFound(Uuid),
}

#[async_trait]
pub trait ActionTemplateRepository: Send + Sync {
    async fn create(
        &self,
        template: ActionTemplate,
        actor: &str,
    ) -> Result<ActionTemplate, ActionTemplateRepositoryError>;
    async fn update(
        &self,
        template: ActionTemplate,
        actor: &str,
    ) -> Result<ActionTemplate, ActionTemplateRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ActionTemplate>, ActionTemplateRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionTemplate>, ActionTemplateRepositoryError>;
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), ActionTemplateRepositoryError>;
}

pub struct PostgresActionTemplateRepository {
    pool: sqlx::PgPool,
}

impl PostgresActionTemplateRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type ActionTemplateRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    serde_json::Value,
    i32,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

fn row_to_template(
    row: ActionTemplateRow,
) -> Result<ActionTemplate, ActionTemplateRepositoryError> {
    let (id, tenant_id, name, description, action_type, config, version, created_at, updated_at) =
        row;
    let action_type = serde_json::from_value(serde_json::Value::String(action_type))
        .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
    Ok(ActionTemplate {
        id,
        tenant_id,
        name,
        description,
        action_type,
        config,
        version,
        created_at,
        updated_at,
    })
}

fn action_type_string(action_type: ActionType) -> Result<String, ActionTemplateRepositoryError> {
    serde_json::to_value(action_type)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| ActionTemplateRepositoryError::Backend("invalid action type".to_string()))
}

#[async_trait]
impl ActionTemplateRepository for PostgresActionTemplateRepository {
    async fn create(
        &self,
        template: ActionTemplate,
        actor: &str,
    ) -> Result<ActionTemplate, ActionTemplateRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        sqlx::query("INSERT INTO action_templates (id, tenant_id, name, description, action_type, config, version, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(template.id)
            .bind(template.tenant_id)
            .bind(&template.name)
            .bind(&template.description)
            .bind(action_type_string(template.action_type)?)
            .bind(&template.config)
            .bind(template.version)
            .bind(template.created_at)
            .bind(template.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        audit(&mut tx, &template, ChangeType::Created, actor, None).await?;
        tx.commit().await.map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        Ok(template)
    }

    async fn update(
        &self,
        mut template: ActionTemplate,
        actor: &str,
    ) -> Result<ActionTemplate, ActionTemplateRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        let existing: Option<ActionTemplateRow> = sqlx::query_as("SELECT id, tenant_id, name, description, action_type, config, version, created_at, updated_at FROM action_templates WHERE id=$1 AND tenant_id=$2")
            .bind(template.id).bind(template.tenant_id).fetch_optional(&mut *tx).await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        let Some(existing) = existing else {
            return Err(ActionTemplateRepositoryError::NotFound(template.id));
        };
        let before = row_to_template(existing)?;
        template.version = before.version + 1;
        template.updated_at = chrono::Utc::now();
        sqlx::query("UPDATE action_templates SET name=$1, description=$2, action_type=$3, config=$4, version=$5, updated_at=$6 WHERE id=$7 AND tenant_id=$8")
            .bind(&template.name).bind(&template.description).bind(action_type_string(template.action_type)?).bind(&template.config).bind(template.version).bind(template.updated_at).bind(template.id).bind(template.tenant_id).execute(&mut *tx).await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        audit(&mut tx, &template, ChangeType::Updated, actor, Some(&before)).await?;
        tx.commit().await.map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        Ok(template)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ActionTemplate>, ActionTemplateRepositoryError> {
        let row: Option<ActionTemplateRow> = sqlx::query_as("SELECT id, tenant_id, name, description, action_type, config, version, created_at, updated_at FROM action_templates WHERE id=$1 AND tenant_id=$2")
            .bind(id).bind(tenant_id).fetch_optional(&self.pool).await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        row.map(row_to_template).transpose()
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionTemplate>, ActionTemplateRepositoryError> {
        let rows: Vec<ActionTemplateRow> = sqlx::query_as("SELECT id, tenant_id, name, description, action_type, config, version, created_at, updated_at FROM action_templates WHERE tenant_id=$1 ORDER BY name")
            .bind(tenant_id).fetch_all(&self.pool).await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        rows.into_iter().map(row_to_template).collect()
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), ActionTemplateRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        let existing: Option<ActionTemplateRow> = sqlx::query_as("SELECT id, tenant_id, name, description, action_type, config, version, created_at, updated_at FROM action_templates WHERE id=$1 AND tenant_id=$2")
            .bind(id).bind(tenant_id).fetch_optional(&mut *tx).await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        let Some(existing) = existing else {
            return Err(ActionTemplateRepositoryError::NotFound(id));
        };
        let before = row_to_template(existing)?;
        sqlx::query("DELETE FROM action_templates WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        audit(&mut tx, &before, ChangeType::Deleted, actor, Some(&before)).await?;
        tx.commit().await.map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    template: &ActionTemplate,
    change_type: ChangeType,
    actor: &str,
    before: Option<&ActionTemplate>,
) -> Result<(), ActionTemplateRepositoryError> {
    record_audit_entry(
        tx,
        &AuditLogEntry {
            id: Uuid::new_v4(),
            tenant_id: template.tenant_id,
            entity_type: "action_template".to_string(),
            entity_id: template.id,
            change_type,
            actor: actor.to_string(),
            before: before.and_then(|value| serde_json::to_value(value).ok()),
            after: serde_json::to_value(template).unwrap_or_default(),
            changed_at: chrono::Utc::now(),
        },
    )
    .await
    .map_err(|e| ActionTemplateRepositoryError::Backend(e.to_string()))
}
