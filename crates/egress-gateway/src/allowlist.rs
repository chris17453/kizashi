#[path = "allowlist_test.rs"]
#[cfg(test)]
pub(crate) mod allowlist_test;

use crate::allowlist_audit_log::{
    record_allowlist_audit_entry, AllowlistAuditLogEntry, AllowlistChangeType,
};
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AllowlistError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Per-tenant domain allowlist (ADR-0021) — owned entirely by Egress Gateway, unlike
/// Triggers/Agents/AnalysisConfig: no other service ever needs to read this, so there's no
/// event-driven-sync case to make; a small local CRUD is the whole story.
#[async_trait]
pub trait AllowlistRepository: Send + Sync {
    /// Empty result means "no allowlist configured" — every destination is allowed
    /// (ADR-0021: opt-in restriction, not default-deny).
    async fn get_domains(&self, tenant_id: &str) -> Result<Vec<String>, AllowlistError>;
    /// `actor` is written to `allowlist_audit_log` in the same transaction as the domain
    /// change (CLAUDE.md §5 — every mutable config entity ships with an audit-log write;
    /// this one had none until ADR-0097).
    async fn set_domains(
        &self,
        tenant_id: &str,
        domains: Vec<String>,
        actor: &str,
    ) -> Result<(), AllowlistError>;
}

/// True if `host` is allowed given a tenant's configured allowlist: an empty allowlist means
/// "no restriction configured," everything is allowed. Otherwise `host` must equal, or be a
/// subdomain of, at least one entry — matched on a real label boundary (`.` prefix), so
/// `zendesk.com` matches `acme.zendesk.com` but never `notzendesk.com`.
pub fn is_host_allowed(allowlist: &[String], host: &str) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    allowlist.iter().any(|domain| host == domain || host.ends_with(&format!(".{domain}")))
}

pub struct PostgresAllowlistRepository {
    pool: sqlx::PgPool,
}

impl PostgresAllowlistRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AllowlistRepository for PostgresAllowlistRepository {
    async fn get_domains(&self, tenant_id: &str) -> Result<Vec<String>, AllowlistError> {
        let row: Option<(Vec<String>,)> =
            sqlx::query_as("SELECT domains FROM tenant_allowlists WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AllowlistError::Backend(e.to_string()))?;
        Ok(row.map(|(domains,)| domains).unwrap_or_default())
    }

    async fn set_domains(
        &self,
        tenant_id: &str,
        domains: Vec<String>,
        actor: &str,
    ) -> Result<(), AllowlistError> {
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|e| AllowlistError::Backend(format!("tenant_id is not a valid UUID: {e}")))?;

        let mut tx = self.pool.begin().await.map_err(|e| AllowlistError::Backend(e.to_string()))?;

        let existing: Option<(Vec<String>,)> =
            sqlx::query_as("SELECT domains FROM tenant_allowlists WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AllowlistError::Backend(e.to_string()))?;
        let before = existing.map(|(domains,)| domains);
        let change_type = if before.is_some() {
            AllowlistChangeType::Updated
        } else {
            AllowlistChangeType::Created
        };

        sqlx::query(
            r#"
            INSERT INTO tenant_allowlists (tenant_id, domains)
            VALUES ($1, $2)
            ON CONFLICT (tenant_id) DO UPDATE SET domains = EXCLUDED.domains
            "#,
        )
        .bind(tenant_id)
        .bind(&domains)
        .execute(&mut *tx)
        .await
        .map_err(|e| AllowlistError::Backend(e.to_string()))?;

        record_allowlist_audit_entry(
            &mut tx,
            &AllowlistAuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: tenant_uuid,
                entity_type: "egress_allowlist".to_string(),
                entity_id: tenant_uuid,
                change_type,
                actor: actor.to_string(),
                before: before.map(|d| serde_json::json!({"domains": d})),
                after: serde_json::json!({"domains": domains}),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| AllowlistError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| AllowlistError::Backend(e.to_string()))?;
        Ok(())
    }
}
