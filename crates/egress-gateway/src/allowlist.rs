#[path = "allowlist_test.rs"]
#[cfg(test)]
pub(crate) mod allowlist_test;

use async_trait::async_trait;
use thiserror::Error;

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
    async fn set_domains(
        &self,
        tenant_id: &str,
        domains: Vec<String>,
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
    ) -> Result<(), AllowlistError> {
        sqlx::query(
            r#"
            INSERT INTO tenant_allowlists (tenant_id, domains)
            VALUES ($1, $2)
            ON CONFLICT (tenant_id) DO UPDATE SET domains = EXCLUDED.domains
            "#,
        )
        .bind(tenant_id)
        .bind(&domains)
        .execute(&self.pool)
        .await
        .map_err(|e| AllowlistError::Backend(e.to_string()))?;
        Ok(())
    }
}
