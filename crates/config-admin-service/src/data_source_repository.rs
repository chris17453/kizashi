#[cfg(test)]
#[path = "data_source_repository_test.rs"]
pub(crate) mod data_source_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::{DataSource, DataSourceKind, DataSourceMode};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DataSourceRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no data source with id {0}")]
    NotFound(Uuid),
}

#[async_trait]
pub trait DataSourceRepository: Send + Sync {
    async fn create(
        &self,
        value: DataSource,
        actor: &str,
    ) -> Result<DataSource, DataSourceRepositoryError>;
    async fn update(
        &self,
        value: DataSource,
        actor: &str,
    ) -> Result<DataSource, DataSourceRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DataSource>, DataSourceRepositoryError>;
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DataSource>, DataSourceRepositoryError>;
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), DataSourceRepositoryError>;
}

pub struct PostgresDataSourceRepository {
    pool: sqlx::PgPool,
}
impl PostgresDataSourceRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type Row = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    serde_json::Value,
    Option<String>,
    bool,
    i32,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

fn data_source_kind_string(value: DataSourceKind) -> Result<String, DataSourceRepositoryError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| DataSourceRepositoryError::Backend("invalid data source kind".to_string()))
}
fn data_source_mode_string(value: DataSourceMode) -> Result<String, DataSourceRepositoryError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| DataSourceRepositoryError::Backend("invalid data source mode".to_string()))
}
fn row_to_value(row: Row) -> Result<DataSource, DataSourceRepositoryError> {
    let (
        id,
        tenant_id,
        name,
        description,
        kind,
        mode,
        connection,
        credential_ref,
        enabled,
        version,
        created_at,
        updated_at,
    ) = row;
    Ok(DataSource {
        id,
        tenant_id,
        name,
        description,
        kind: serde_json::from_value(serde_json::json!(kind))
            .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?,
        mode: serde_json::from_value(serde_json::json!(mode))
            .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?,
        connection,
        credential_ref,
        enabled,
        version,
        created_at,
        updated_at,
    })
}

#[async_trait]
impl DataSourceRepository for PostgresDataSourceRepository {
    async fn create(
        &self,
        value: DataSource,
        actor: &str,
    ) -> Result<DataSource, DataSourceRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        sqlx::query("INSERT INTO data_sources (id,tenant_id,name,description,kind,mode,connection,credential_ref,enabled,version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(value.id).bind(value.tenant_id).bind(&value.name).bind(&value.description).bind(data_source_kind_string(value.kind)?).bind(data_source_mode_string(value.mode)?).bind(&value.connection).bind(&value.credential_ref).bind(value.enabled).bind(value.version).bind(value.created_at).bind(value.updated_at).execute(&mut *tx).await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        audit(&mut tx, &value, ChangeType::Created, actor, None).await?;
        tx.commit().await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        Ok(value)
    }
    async fn update(
        &self,
        mut value: DataSource,
        actor: &str,
    ) -> Result<DataSource, DataSourceRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        let row: Option<Row> = sqlx::query_as("SELECT id,tenant_id,name,description,kind,mode,connection,credential_ref,enabled,version,created_at,updated_at FROM data_sources WHERE id=$1 AND tenant_id=$2").bind(value.id).bind(value.tenant_id).fetch_optional(&mut *tx).await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        let before = row
            .map(row_to_value)
            .transpose()?
            .ok_or(DataSourceRepositoryError::NotFound(value.id))?;
        value.version = before.version + 1;
        value.created_at = before.created_at;
        value.updated_at = chrono::Utc::now();
        sqlx::query("UPDATE data_sources SET name=$1,description=$2,kind=$3,mode=$4,connection=$5,credential_ref=$6,enabled=$7,version=$8,updated_at=$9 WHERE id=$10 AND tenant_id=$11").bind(&value.name).bind(&value.description).bind(data_source_kind_string(value.kind)?).bind(data_source_mode_string(value.mode)?).bind(&value.connection).bind(&value.credential_ref).bind(value.enabled).bind(value.version).bind(value.updated_at).bind(value.id).bind(value.tenant_id).execute(&mut *tx).await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        audit(&mut tx, &value, ChangeType::Updated, actor, Some(&before)).await?;
        tx.commit().await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        Ok(value)
    }
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DataSource>, DataSourceRepositoryError> {
        sqlx::query_as("SELECT id,tenant_id,name,description,kind,mode,connection,credential_ref,enabled,version,created_at,updated_at FROM data_sources WHERE id=$1 AND tenant_id=$2").bind(id).bind(tenant_id).fetch_optional(&self.pool).await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?.map(row_to_value).transpose()
    }
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DataSource>, DataSourceRepositoryError> {
        sqlx::query_as("SELECT id,tenant_id,name,description,kind,mode,connection,credential_ref,enabled,version,created_at,updated_at FROM data_sources WHERE tenant_id=$1 ORDER BY name").bind(tenant_id).fetch_all(&self.pool).await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?.into_iter().map(row_to_value).collect()
    }
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), DataSourceRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        let row: Option<Row> = sqlx::query_as("SELECT id,tenant_id,name,description,kind,mode,connection,credential_ref,enabled,version,created_at,updated_at FROM data_sources WHERE id=$1 AND tenant_id=$2").bind(id).bind(tenant_id).fetch_optional(&mut *tx).await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        let value =
            row.map(row_to_value).transpose()?.ok_or(DataSourceRepositoryError::NotFound(id))?;
        sqlx::query("DELETE FROM data_sources WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))?;
        audit(&mut tx, &value, ChangeType::Deleted, actor, Some(&value)).await?;
        tx.commit().await.map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))
    }
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    value: &DataSource,
    change_type: ChangeType,
    actor: &str,
    before: Option<&DataSource>,
) -> Result<(), DataSourceRepositoryError> {
    record_audit_entry(
        tx,
        &AuditLogEntry {
            id: Uuid::new_v4(),
            tenant_id: value.tenant_id,
            entity_type: "data_source".to_string(),
            entity_id: value.id,
            change_type,
            actor: actor.to_string(),
            before: before.map(|v| serde_json::to_value(v).unwrap_or_default()),
            after: serde_json::to_value(value).unwrap_or_default(),
            changed_at: chrono::Utc::now(),
        },
    )
    .await
    .map_err(|e| DataSourceRepositoryError::Backend(e.to_string()))
}
