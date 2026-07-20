#[path = "tenant_branding_repository_test.rs"]
#[cfg(test)]
pub(crate) mod tenant_branding_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TenantBrandingRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// White-label overrides for a tenant (spec §1: "white-labelable"). Every field is optional —
/// `None` means "use the platform default", not "misconfigured" — so a tenant that never
/// touches branding renders identically to today, no migration-day surprise.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TenantBranding {
    pub product_name: Option<String>,
    pub logo_url: Option<String>,
    pub accent_color: Option<String>,
}

/// Read is by workspace *name*, not id — the one caller that needs this before anything else
/// (the login page) only ever has the name the user typed, same reasoning as
/// `TenantRepository::id_for_name`. Write is audit-logged (CLAUDE.md §5) same as every other
/// admin-mutable config in this platform.
#[async_trait]
pub trait TenantBrandingRepository: Send + Sync {
    async fn branding_for_name(
        &self,
        name: &str,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError>;

    /// Used by the authenticated Settings page, which only ever has a `tenant_id` (from the
    /// session), never the workspace name — the login page's `branding_for_name` isn't reachable
    /// from there.
    async fn branding_for_id(
        &self,
        id: Uuid,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError>;

    async fn update_branding(
        &self,
        tenant_id: Uuid,
        branding: TenantBranding,
        actor: &str,
    ) -> Result<(), TenantBrandingRepositoryError>;
}

pub struct PostgresTenantBrandingRepository {
    pool: sqlx::PgPool,
}

impl PostgresTenantBrandingRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantBrandingRepository for PostgresTenantBrandingRepository {
    async fn branding_for_name(
        &self,
        name: &str,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError> {
        let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT product_name, logo_url, accent_color FROM tenants WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;

        Ok(row.map(|(product_name, logo_url, accent_color)| TenantBranding {
            product_name,
            logo_url,
            accent_color,
        }))
    }

    async fn branding_for_id(
        &self,
        id: Uuid,
    ) -> Result<Option<TenantBranding>, TenantBrandingRepositoryError> {
        let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT product_name, logo_url, accent_color FROM tenants WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;

        Ok(row.map(|(product_name, logo_url, accent_color)| TenantBranding {
            product_name,
            logo_url,
            accent_color,
        }))
    }

    async fn update_branding(
        &self,
        tenant_id: Uuid,
        branding: TenantBranding,
        actor: &str,
    ) -> Result<(), TenantBrandingRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;

        let before: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT product_name, logo_url, accent_color FROM tenants WHERE id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;
        let before_branding = before.map(|(product_name, logo_url, accent_color)| TenantBranding {
            product_name,
            logo_url,
            accent_color,
        });

        sqlx::query(
            "UPDATE tenants SET product_name = $1, logo_url = $2, accent_color = $3 WHERE id = $4",
        )
        .bind(&branding.product_name)
        .bind(&branding.logo_url)
        .bind(&branding.accent_color)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "tenant_branding".to_string(),
                entity_id: tenant_id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: before_branding.map(|b| serde_json::to_value(b).unwrap_or_default()),
                after: serde_json::to_value(&branding).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| TenantBrandingRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
