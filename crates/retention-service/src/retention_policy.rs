#[path = "retention_policy_test.rs"]
#[cfg(test)]
pub(crate) mod retention_policy_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// v1 only enforces `Raw` (ADR-0011) — `Normalized`/`Event` are structurally represented so
/// the policy schema doesn't need a migration when those archival paths are built, but
/// `sweep` (see sweep.rs) only acts on `Raw` policies today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    Raw,
    Normalized,
    Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub data_class: DataClass,
    pub ttl_days: i32,
    pub enabled: bool,
}

#[derive(Debug, Error)]
pub enum RetentionPolicyRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no retention policy with id {0}")]
    NotFound(Uuid),
}

/// CRUD for RetentionPolicy, this service's own Postgres schema (ADR-0011). Every
/// create/update writes one audit_log row in the same transaction as the entity change
/// (CLAUDE.md §5), same pattern as config-admin-service's repositories.
#[async_trait]
pub trait RetentionPolicyRepository: Send + Sync {
    async fn create(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError>;
    async fn update(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid)
        -> Result<(), RetentionPolicyRepositoryError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionPolicyRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError>;
    /// All enabled policies across all tenants — what `sweep` iterates over.
    async fn list_all_enabled(
        &self,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError>;
}

pub struct PostgresRetentionPolicyRepository {
    pool: sqlx::PgPool,
}

impl PostgresRetentionPolicyRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type PolicyRow = (Uuid, Uuid, sqlx::types::Json<DataClass>, i32, bool);

fn row_to_policy(row: PolicyRow) -> RetentionPolicy {
    let (id, tenant_id, data_class, ttl_days, enabled) = row;
    RetentionPolicy { id, tenant_id, data_class: data_class.0, ttl_days, enabled }
}

#[async_trait]
impl RetentionPolicyRepository for PostgresRetentionPolicyRepository {
    async fn create(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            "INSERT INTO retention_policies (id, tenant_id, data_class, ttl_days, enabled) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(policy.id)
        .bind(policy.tenant_id)
        .bind(sqlx::types::Json(policy.data_class))
        .bind(policy.ttl_days)
        .bind(policy.enabled)
        .execute(&mut *tx)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: policy.tenant_id,
                entity_type: "retention_policy".to_string(),
                entity_id: policy.id,
                change_type: ChangeType::Created,
                actor: policy.tenant_id.to_string(),
                before: None,
                after: serde_json::to_value(&policy).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;
        Ok(policy)
    }

    async fn update(
        &self,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPolicyRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        let existing: Option<PolicyRow> = sqlx::query_as(
            "SELECT id, tenant_id, data_class, ttl_days, enabled FROM retention_policies WHERE id = $1 AND tenant_id = $2",
        )
        .bind(policy.id)
        .bind(policy.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(RetentionPolicyRepositoryError::NotFound(policy.id));
        };
        let before = row_to_policy(existing);

        sqlx::query(
            "UPDATE retention_policies SET data_class = $1, ttl_days = $2, enabled = $3 WHERE id = $4 AND tenant_id = $5",
        )
        .bind(sqlx::types::Json(policy.data_class))
        .bind(policy.ttl_days)
        .bind(policy.enabled)
        .bind(policy.id)
        .bind(policy.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: policy.tenant_id,
                entity_type: "retention_policy".to_string(),
                entity_id: policy.id,
                change_type: ChangeType::Updated,
                actor: policy.tenant_id.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&policy).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;
        Ok(policy)
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), RetentionPolicyRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        let existing: Option<PolicyRow> = sqlx::query_as(
            "SELECT id, tenant_id, data_class, ttl_days, enabled FROM retention_policies WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(RetentionPolicyRepositoryError::NotFound(id));
        };
        let before = row_to_policy(existing);

        sqlx::query("DELETE FROM retention_policies WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "retention_policy".to_string(),
                entity_id: id,
                change_type: ChangeType::Deleted,
                actor: tenant_id.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::Value::Null,
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<RetentionPolicy>, RetentionPolicyRepositoryError> {
        let row: Option<PolicyRow> = sqlx::query_as(
            "SELECT id, tenant_id, data_class, ttl_days, enabled FROM retention_policies WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_policy))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError> {
        let rows: Vec<PolicyRow> = sqlx::query_as(
            "SELECT id, tenant_id, data_class, ttl_days, enabled FROM retention_policies WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_policy).collect())
    }

    async fn list_all_enabled(
        &self,
    ) -> Result<Vec<RetentionPolicy>, RetentionPolicyRepositoryError> {
        let rows: Vec<PolicyRow> = sqlx::query_as(
            "SELECT id, tenant_id, data_class, ttl_days, enabled FROM retention_policies WHERE enabled = TRUE",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RetentionPolicyRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_policy).collect())
    }
}
