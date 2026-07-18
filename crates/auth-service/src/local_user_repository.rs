#[path = "local_user_repository_test.rs"]
#[cfg(test)]
pub(crate) mod local_user_repository_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum LocalUserRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub password_hash: String,
}

/// Local login credential store (spec §8: "local login... hashed credentials"). Scoped by
/// tenant so the same username can exist independently across tenants without collision.
#[async_trait]
pub trait LocalUserRepository: Send + Sync {
    async fn find_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<LocalUser>, LocalUserRepositoryError>;
}

pub struct PostgresLocalUserRepository {
    pool: sqlx::PgPool,
}

impl PostgresLocalUserRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LocalUserRepository for PostgresLocalUserRepository {
    async fn find_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<LocalUser>, LocalUserRepositoryError> {
        let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
            "SELECT id, tenant_id, username, password_hash FROM local_users WHERE tenant_id = $1 AND username = $2",
        )
        .bind(tenant_id)
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        Ok(row.map(|(id, tenant_id, username, password_hash)| LocalUser {
            id,
            tenant_id,
            username,
            password_hash,
        }))
    }
}
