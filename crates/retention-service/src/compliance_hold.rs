#[path = "compliance_hold_test.rs"]
#[cfg(test)]
pub(crate) mod compliance_hold_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use crate::retention_policy::DataClass;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceHold {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub data_class: DataClass,
    pub reason: String,
    pub active: bool,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum ComplianceHoldRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no compliance hold with id {0}")]
    NotFound(Uuid),
}

#[async_trait]
pub trait ComplianceHoldRepository: Send + Sync {
    async fn create(
        &self,
        hold: ComplianceHold,
        actor: &str,
    ) -> Result<ComplianceHold, ComplianceHoldRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ComplianceHold>, ComplianceHoldRepositoryError>;
    async fn has_active(
        &self,
        tenant_id: Uuid,
        data_class: DataClass,
    ) -> Result<bool, ComplianceHoldRepositoryError>;
    async fn release(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<ComplianceHold, ComplianceHoldRepositoryError>;
}

pub struct PostgresComplianceHoldRepository {
    pool: sqlx::PgPool,
}

impl PostgresComplianceHoldRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type HoldRow = (
    Uuid,
    Uuid,
    sqlx::types::Json<DataClass>,
    String,
    bool,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

fn row_to_hold(row: HoldRow) -> ComplianceHold {
    let (id, tenant_id, data_class, reason, active, created_by, created_at, released_at) = row;
    ComplianceHold {
        id,
        tenant_id,
        data_class: data_class.0,
        reason,
        active,
        created_by,
        created_at,
        released_at,
    }
}

#[async_trait]
impl ComplianceHoldRepository for PostgresComplianceHoldRepository {
    async fn create(
        &self,
        hold: ComplianceHold,
        actor: &str,
    ) -> Result<ComplianceHold, ComplianceHoldRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        sqlx::query("INSERT INTO compliance_holds (id, tenant_id, data_class, reason, active, created_by, created_at, released_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(hold.id).bind(hold.tenant_id).bind(sqlx::types::Json(hold.data_class)).bind(&hold.reason).bind(hold.active).bind(&hold.created_by).bind(hold.created_at).bind(hold.released_at)
            .execute(&mut *tx).await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: hold.tenant_id,
                entity_type: "compliance_hold".into(),
                entity_id: hold.id,
                change_type: ChangeType::Created,
                actor: actor.into(),
                before: None,
                after: serde_json::to_value(&hold).unwrap_or_default(),
                changed_at: Utc::now(),
            },
        )
        .await
        .map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        Ok(hold)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ComplianceHold>, ComplianceHoldRepositoryError> {
        let rows: Vec<HoldRow> = sqlx::query_as("SELECT id, tenant_id, data_class, reason, active, created_by, created_at, released_at FROM compliance_holds WHERE tenant_id = $1 ORDER BY active DESC, created_at DESC").bind(tenant_id).fetch_all(&self.pool).await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_hold).collect())
    }

    async fn has_active(
        &self,
        tenant_id: Uuid,
        data_class: DataClass,
    ) -> Result<bool, ComplianceHoldRepositoryError> {
        let row: Option<(bool,)> = sqlx::query_as("SELECT TRUE FROM compliance_holds WHERE tenant_id = $1 AND data_class = $2 AND active = TRUE LIMIT 1").bind(tenant_id).bind(sqlx::types::Json(data_class)).fetch_optional(&self.pool).await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        Ok(row.is_some())
    }

    async fn release(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<ComplianceHold, ComplianceHoldRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        let row: Option<HoldRow> = sqlx::query_as("SELECT id, tenant_id, data_class, reason, active, created_by, created_at, released_at FROM compliance_holds WHERE id = $1 AND tenant_id = $2").bind(id).bind(tenant_id).fetch_optional(&mut *tx).await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        let Some(row) = row else {
            return Err(ComplianceHoldRepositoryError::NotFound(id));
        };
        let before = row_to_hold(row);
        let released =
            ComplianceHold { active: false, released_at: Some(Utc::now()), ..before.clone() };
        sqlx::query("UPDATE compliance_holds SET active = FALSE, released_at = $1 WHERE id = $2 AND tenant_id = $3").bind(released.released_at).bind(id).bind(tenant_id).execute(&mut *tx).await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "compliance_hold".into(),
                entity_id: id,
                change_type: ChangeType::Updated,
                actor: actor.into(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&released).unwrap_or_default(),
                changed_at: Utc::now(),
            },
        )
        .await
        .map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| ComplianceHoldRepositoryError::Backend(e.to_string()))?;
        Ok(released)
    }
}
