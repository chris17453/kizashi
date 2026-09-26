#[cfg(test)]
#[path = "pipeline_definition_repository_test.rs"]
pub(crate) mod pipeline_definition_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::{PipelineDefinition, PipelineMode};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PipelineDefinitionRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no pipeline definition with id {0}")]
    NotFound(Uuid),
}

#[async_trait]
pub trait PipelineDefinitionRepository: Send + Sync {
    async fn create(
        &self,
        value: PipelineDefinition,
        actor: &str,
    ) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError>;
    async fn update(
        &self,
        value: PipelineDefinition,
        actor: &str,
    ) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PipelineDefinition>, PipelineDefinitionRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<PipelineDefinition>, PipelineDefinitionRepositoryError>;
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), PipelineDefinitionRepositoryError>;
}

pub struct PostgresPipelineDefinitionRepository {
    pool: sqlx::PgPool,
}
impl PostgresPipelineDefinitionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type Row = (
    Uuid,
    Uuid,
    String,
    String,
    Uuid,
    Option<Uuid>,
    String,
    serde_json::Value,
    bool,
    i32,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);
fn row(value: Row) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError> {
    let (
        id,
        tenant_id,
        name,
        description,
        data_source_id,
        target_object_type_id,
        mode,
        steps,
        enabled,
        version,
        created_at,
        updated_at,
    ) = value;
    Ok(PipelineDefinition {
        id,
        tenant_id,
        name,
        description,
        data_source_id,
        target_object_type_id,
        mode: serde_json::from_value(serde_json::json!(mode))
            .map_err(|e| PipelineDefinitionRepositoryError::Backend(e.to_string()))?,
        steps,
        enabled,
        version,
        created_at,
        updated_at,
    })
}
fn mode(value: PipelineMode) -> Result<String, PipelineDefinitionRepositoryError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| {
            PipelineDefinitionRepositoryError::Backend("invalid pipeline mode".to_string())
        })
}

#[async_trait]
impl PipelineDefinitionRepository for PostgresPipelineDefinitionRepository {
    async fn create(
        &self,
        value: PipelineDefinition,
        actor: &str,
    ) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        sqlx::query("INSERT INTO pipeline_definitions (id,tenant_id,name,description,data_source_id,target_object_type_id,mode,steps,enabled,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(value.id).bind(value.tenant_id).bind(&value.name).bind(&value.description).bind(value.data_source_id).bind(value.target_object_type_id).bind(mode(value.mode)?).bind(&value.steps).bind(value.enabled).bind(value.version).bind(value.created_at).bind(value.updated_at).execute(&mut *tx).await.map_err(backend)?;
        audit(&mut tx, &value, ChangeType::Created, actor, None).await?;
        tx.commit().await.map_err(backend)?;
        Ok(value)
    }
    async fn update(
        &self,
        mut value: PipelineDefinition,
        actor: &str,
    ) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let before = fetch(&mut *tx, value.tenant_id, value.id)
            .await?
            .ok_or(PipelineDefinitionRepositoryError::NotFound(value.id))?;
        value.version = before.version + 1;
        value.created_at = before.created_at;
        value.updated_at = chrono::Utc::now();
        sqlx::query("UPDATE pipeline_definitions SET name=$1,description=$2,data_source_id=$3,target_object_type_id=$4,mode=$5,steps=$6,enabled=$7,version=$8,updated_at=$9 WHERE id=$10 AND tenant_id=$11")
            .bind(&value.name).bind(&value.description).bind(value.data_source_id).bind(value.target_object_type_id).bind(mode(value.mode)?).bind(&value.steps).bind(value.enabled).bind(value.version).bind(value.updated_at).bind(value.id).bind(value.tenant_id).execute(&mut *tx).await.map_err(backend)?;
        audit(&mut tx, &value, ChangeType::Updated, actor, Some(&before)).await?;
        tx.commit().await.map_err(backend)?;
        Ok(value)
    }
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PipelineDefinition>, PipelineDefinitionRepositoryError> {
        fetch(&self.pool, tenant_id, id).await
    }
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<PipelineDefinition>, PipelineDefinitionRepositoryError> {
        sqlx::query_as("SELECT id,tenant_id,name,description,data_source_id,target_object_type_id,mode,steps,enabled,version,created_at,updated_at FROM pipeline_definitions WHERE tenant_id=$1 ORDER BY name").bind(tenant_id).fetch_all(&self.pool).await.map_err(backend)?.into_iter().map(row).collect()
    }
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), PipelineDefinitionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let value = fetch(&mut *tx, tenant_id, id)
            .await?
            .ok_or(PipelineDefinitionRepositoryError::NotFound(id))?;
        sqlx::query("DELETE FROM pipeline_definitions WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        audit(&mut tx, &value, ChangeType::Deleted, actor, Some(&value)).await?;
        tx.commit().await.map_err(backend)
    }
}

fn backend(error: sqlx::Error) -> PipelineDefinitionRepositoryError {
    PipelineDefinitionRepositoryError::Backend(error.to_string())
}

async fn fetch<'e, E>(
    executor: E,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<PipelineDefinition>, PipelineDefinitionRepositoryError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as("SELECT id,tenant_id,name,description,data_source_id,target_object_type_id,mode,steps,enabled,version,created_at,updated_at FROM pipeline_definitions WHERE id=$1 AND tenant_id=$2")
        .bind(id).bind(tenant_id).fetch_optional(executor).await.map_err(backend)?.map(row).transpose()
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    value: &PipelineDefinition,
    change_type: ChangeType,
    actor: &str,
    before: Option<&PipelineDefinition>,
) -> Result<(), PipelineDefinitionRepositoryError> {
    record_audit_entry(
        tx,
        &AuditLogEntry {
            id: Uuid::new_v4(),
            tenant_id: value.tenant_id,
            entity_type: "pipeline_definition".to_string(),
            entity_id: value.id,
            change_type,
            actor: actor.to_string(),
            before: before.map(|value| serde_json::to_value(value).unwrap_or_default()),
            after: serde_json::to_value(value).unwrap_or_default(),
            changed_at: chrono::Utc::now(),
        },
    )
    .await
    .map_err(|error| PipelineDefinitionRepositoryError::Backend(error.to_string()))
}
