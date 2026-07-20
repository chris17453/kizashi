#[path = "audit_log_test.rs"]
#[cfg(test)]
pub(crate) mod audit_log_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuditLogError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub change_type: ChangeType,
    pub actor: String,
    pub before: Option<serde_json::Value>,
    pub after: serde_json::Value,
    pub changed_at: DateTime<Utc>,
}

/// Writes one immutable audit row in the *same transaction* as the entity mutation it records
/// (CLAUDE.md §5) — mirrors config-admin-service's `record_audit_entry` exactly, applied here
/// to user-management mutations (ADR-0016 follow-up: role assignment/user CRUD).
pub async fn record_audit_entry(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entry: &AuditLogEntry,
) -> Result<(), AuditLogError> {
    sqlx::query(
        r#"
        INSERT INTO auth_audit_log
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
    .map_err(|e| AuditLogError::Backend(e.to_string()))?;
    Ok(())
}

/// Read-only access to the audit trail — no transactional-sharing constraint, so a normal
/// mockable trait.
#[async_trait]
pub trait AuditLogReader: Send + Sync {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError>;

    /// Chronological "recent activity" feed across ALL entities for a tenant — deliberately
    /// DESC (most-recent-first), the opposite of `list_for_entity`'s ASC, since this backs a
    /// "show me every admin action in the last N days" compliance view rather than a single
    /// entity's history read top-to-bottom. `before` is an exclusive cursor (strictly older than)
    /// for simple keyset pagination: pass the `changed_at` of the last row seen to fetch the next
    /// page.
    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError>;
}

pub struct PostgresAuditLogReader {
    pool: sqlx::PgPool,
}

impl PostgresAuditLogReader {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type AuditRow = (
    Uuid,
    Uuid,
    String,
    Uuid,
    sqlx::types::Json<ChangeType>,
    String,
    Option<serde_json::Value>,
    serde_json::Value,
    DateTime<Utc>,
);

fn row_to_entry(row: AuditRow) -> AuditLogEntry {
    let (id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at) =
        row;
    AuditLogEntry {
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
impl AuditLogReader for PostgresAuditLogReader {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at
            FROM auth_audit_log
            WHERE tenant_id = $1 AND entity_id = $2
            ORDER BY changed_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuditLogError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_entry).collect())
    }

    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogError> {
        // Two query variants rather than a sentinel timestamp: a sentinel (e.g. `DateTime::MAX`)
        // would silently misbehave if a future clock-skewed row ever exceeded it, whereas
        // conditionally binding `$2` is unambiguous and just as simple in sqlx.
        let rows: Vec<AuditRow> = if let Some(before) = before {
            sqlx::query_as(
                r#"
                SELECT id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at
                FROM auth_audit_log
                WHERE tenant_id = $1 AND changed_at < $2
                ORDER BY changed_at DESC
                LIMIT $3
                "#,
            )
            .bind(tenant_id)
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at
                FROM auth_audit_log
                WHERE tenant_id = $1
                ORDER BY changed_at DESC
                LIMIT $2
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| AuditLogError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_entry).collect())
    }
}
