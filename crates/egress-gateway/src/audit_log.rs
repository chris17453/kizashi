#[path = "audit_log_test.rs"]
#[cfg(test)]
pub(crate) mod audit_log_test;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditLogError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// One CONNECT attempt (ADR-0021) — `tenant_id`/`connector_id` come from `Proxy-Authorization`
/// (`"unknown"` if absent/malformed, not rejected in v1), `destination_host`/`_port` from the
/// CONNECT target. Append-only, like every other audit log in this system (CLAUDE.md §5) —
/// this repository has no update/delete method at all, not just a convention against using one.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditLogEntry {
    pub tenant_id: String,
    pub connector_id: String,
    pub destination_host: String,
    pub destination_port: u16,
    pub allowed: bool,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn record(&self, entry: AuditLogEntry) -> Result<(), AuditLogError>;
}

pub struct PostgresAuditLogRepository {
    pool: sqlx::PgPool,
}

impl PostgresAuditLogRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditLogRepository for PostgresAuditLogRepository {
    async fn record(&self, entry: AuditLogEntry) -> Result<(), AuditLogError> {
        sqlx::query(
            r#"
            INSERT INTO egress_audit_log
                (tenant_id, connector_id, destination_host, destination_port, allowed, occurred_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&entry.tenant_id)
        .bind(&entry.connector_id)
        .bind(&entry.destination_host)
        .bind(entry.destination_port as i32)
        .bind(entry.allowed)
        .bind(entry.occurred_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AuditLogError::Backend(e.to_string()))?;
        Ok(())
    }
}
