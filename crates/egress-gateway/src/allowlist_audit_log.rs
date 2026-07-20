#[path = "allowlist_audit_log_test.rs"]
#[cfg(test)]
pub(crate) mod allowlist_audit_log_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AllowlistAuditLogError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowlistChangeType {
    Created,
    Updated,
}

/// Same shape as `config_admin_service::audit_log::AuditLogEntry` (id/tenant_id/entity_type/
/// entity_id/change_type/actor/before/after/changed_at) so it can be read back through the
/// same generic `GET /v1/audit-log/:entity_id` route the Console UI's `HttpAuditLogClient`
/// already calls for config-admin-service/retention-service/auth-service -- no new UI client
/// type needed, unlike `IngestionGatewayApiKeyAuditLogClient` (ADR-0094), which had to differ
/// because ingestion-gateway's route shape didn't match. `entity_id` is the tenant's own id:
/// the allowlist is a singleton-per-tenant resource, same convention `AnalysisConfig` uses.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AllowlistAuditLogEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub change_type: AllowlistChangeType,
    pub actor: String,
    pub before: Option<serde_json::Value>,
    pub after: serde_json::Value,
    pub changed_at: DateTime<Utc>,
}

/// Writes one immutable audit row in the *same transaction* as the allowlist mutation it
/// records (CLAUDE.md §5), same pattern as `config_admin_service::audit_log::record_audit_entry`.
pub async fn record_allowlist_audit_entry(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entry: &AllowlistAuditLogEntry,
) -> Result<(), AllowlistAuditLogError> {
    sqlx::query(
        r#"
        INSERT INTO allowlist_audit_log
            (id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(entry.id)
    .bind(entry.tenant_id)
    .bind(&entry.entity_type)
    .bind(entry.entity_id)
    .bind(sqlx::types::Json(entry.change_type))
    .bind(&entry.actor)
    .bind(&entry.before)
    .bind(&entry.after)
    .bind(entry.changed_at)
    .execute(&mut **tx)
    .await
    .map_err(|e| AllowlistAuditLogError::Backend(e.to_string()))?;
    Ok(())
}

#[async_trait]
pub trait AllowlistAuditLogReader: Send + Sync {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AllowlistAuditLogEntry>, AllowlistAuditLogError>;
}

pub struct PostgresAllowlistAuditLogReader {
    pool: sqlx::PgPool,
}

impl PostgresAllowlistAuditLogReader {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type AuditRow = (
    Uuid,
    Uuid,
    String,
    Uuid,
    sqlx::types::Json<AllowlistChangeType>,
    String,
    Option<serde_json::Value>,
    serde_json::Value,
    DateTime<Utc>,
);

fn row_to_entry(row: AuditRow) -> AllowlistAuditLogEntry {
    let (id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at) =
        row;
    AllowlistAuditLogEntry {
        id,
        tenant_id,
        entity_type,
        entity_id,
        change_type: change_type.0,
        actor,
        before,
        after,
        changed_at,
    }
}

#[async_trait]
impl AllowlistAuditLogReader for PostgresAllowlistAuditLogReader {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AllowlistAuditLogEntry>, AllowlistAuditLogError> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at
            FROM allowlist_audit_log
            WHERE tenant_id = $1 AND entity_id = $2
            ORDER BY changed_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AllowlistAuditLogError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_entry).collect())
    }
}
