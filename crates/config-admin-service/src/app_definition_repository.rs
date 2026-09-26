use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::AppDefinition;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AppDefinitionRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no app definition with id {0}")]
    NotFound(Uuid),
}
#[async_trait]
pub trait AppDefinitionRepository: Send + Sync {
    async fn create(
        &self,
        value: AppDefinition,
        actor: &str,
    ) -> Result<AppDefinition, AppDefinitionRepositoryError>;
    async fn update(
        &self,
        value: AppDefinition,
        actor: &str,
    ) -> Result<AppDefinition, AppDefinitionRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<AppDefinition>, AppDefinitionRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<AppDefinition>, AppDefinitionRepositoryError>;
}
pub struct PostgresAppDefinitionRepository {
    pool: sqlx::PgPool,
}
impl PostgresAppDefinitionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}
type Row = (
    Uuid,
    Uuid,
    String,
    String,
    Option<Uuid>,
    serde_json::Value,
    bool,
    i32,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);
fn row(value: Row) -> AppDefinition {
    let (
        id,
        tenant_id,
        name,
        description,
        model_type_id,
        blocks,
        enabled,
        version,
        created_at,
        updated_at,
    ) = value;
    AppDefinition {
        id,
        tenant_id,
        name,
        description,
        model_type_id,
        blocks,
        enabled,
        version,
        created_at,
        updated_at,
    }
}
fn error(value: sqlx::Error) -> AppDefinitionRepositoryError {
    AppDefinitionRepositoryError::Backend(value.to_string())
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    value: &AppDefinition,
    change_type: ChangeType,
    actor: &str,
    before: Option<&AppDefinition>,
) -> Result<(), AppDefinitionRepositoryError> {
    record_audit_entry(
        tx,
        &AuditLogEntry {
            id: Uuid::new_v4(),
            tenant_id: value.tenant_id,
            entity_type: "app_definition".to_string(),
            entity_id: value.id,
            change_type,
            actor: actor.to_string(),
            before: before.map(|value| serde_json::to_value(value).unwrap_or_default()),
            after: serde_json::to_value(value).unwrap_or_default(),
            changed_at: chrono::Utc::now(),
        },
    )
    .await
    .map_err(|value| AppDefinitionRepositoryError::Backend(value.to_string()))
}
#[async_trait]
impl AppDefinitionRepository for PostgresAppDefinitionRepository {
    async fn create(
        &self,
        value: AppDefinition,
        actor: &str,
    ) -> Result<AppDefinition, AppDefinitionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(error)?;
        sqlx::query("INSERT INTO app_definitions (id,tenant_id,name,description,model_type_id,blocks,enabled,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(value.id).bind(value.tenant_id).bind(&value.name).bind(&value.description).bind(value.model_type_id).bind(&value.blocks).bind(value.enabled).bind(value.version).bind(value.created_at).bind(value.updated_at).execute(&mut *tx).await.map_err(error)?;
        audit(&mut tx, &value, ChangeType::Created, actor, None).await?;
        tx.commit().await.map_err(error)?;
        Ok(value)
    }
    async fn update(
        &self,
        mut value: AppDefinition,
        actor: &str,
    ) -> Result<AppDefinition, AppDefinitionRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(error)?;
        let prior: Option<Row> = sqlx::query_as("SELECT id,tenant_id,name,description,model_type_id,blocks,enabled,version,created_at,updated_at FROM app_definitions WHERE tenant_id=$1 AND id=$2").bind(value.tenant_id).bind(value.id).fetch_optional(&mut *tx).await.map_err(error)?;
        let before = prior.map(row).ok_or(AppDefinitionRepositoryError::NotFound(value.id))?;
        value.version = before.version + 1;
        value.created_at = before.created_at;
        value.updated_at = chrono::Utc::now();
        sqlx::query("UPDATE app_definitions SET name=$1,description=$2,model_type_id=$3,blocks=$4,enabled=$5,version=$6,updated_at=$7 WHERE tenant_id=$8 AND id=$9").bind(&value.name).bind(&value.description).bind(value.model_type_id).bind(&value.blocks).bind(value.enabled).bind(value.version).bind(value.updated_at).bind(value.tenant_id).bind(value.id).execute(&mut *tx).await.map_err(error)?;
        audit(&mut tx, &value, ChangeType::Updated, actor, Some(&before)).await?;
        tx.commit().await.map_err(error)?;
        Ok(value)
    }
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<AppDefinition>, AppDefinitionRepositoryError> {
        sqlx::query_as("SELECT id,tenant_id,name,description,model_type_id,blocks,enabled,version,created_at,updated_at FROM app_definitions WHERE tenant_id=$1 AND id=$2").bind(tenant_id).bind(id).fetch_optional(&self.pool).await.map_err(error).map(|value: Option<Row>| value.map(row))
    }
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<AppDefinition>, AppDefinitionRepositoryError> {
        sqlx::query_as("SELECT id,tenant_id,name,description,model_type_id,blocks,enabled,version,created_at,updated_at FROM app_definitions WHERE tenant_id=$1 ORDER BY name").bind(tenant_id).fetch_all(&self.pool).await.map_err(error).map(|values: Vec<Row>| values.into_iter().map(row).collect())
    }
}
