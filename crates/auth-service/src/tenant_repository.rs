#[path = "tenant_repository_test.rs"]
#[cfg(test)]
pub(crate) mod tenant_repository_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TenantRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Resolves a human-typed workspace name to its `tenant_id` (spec §8 tenants are otherwise
/// bare UUIDs everywhere else in the system — this is the one place a person, not a service,
/// has to identify one, so it needs a name people can actually type).
#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn id_for_name(&self, name: &str) -> Result<Option<Uuid>, TenantRepositoryError>;
}

pub struct PostgresTenantRepository {
    pool: sqlx::PgPool,
}

impl PostgresTenantRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantRepository for PostgresTenantRepository {
    async fn id_for_name(&self, name: &str) -> Result<Option<Uuid>, TenantRepositoryError> {
        let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM tenants WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;

        Ok(row.map(|(id,)| id))
    }
}
