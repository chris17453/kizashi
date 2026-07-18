#[path = "api_key_store_test.rs"]
#[cfg(test)]
pub(crate) mod api_key_store_test;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApiKeyStoreError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Resolves an API key presented by a connector/agent to the tenant it authenticates as
/// (spec §8: "gateway layer: auth context scopes all downstream queries"). Keys are stored
/// and looked up by their SHA-256 hash — the plaintext key is never persisted (CLAUDE.md §5).
#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    async fn tenant_for_key(&self, api_key: &str) -> Result<Option<Uuid>, ApiKeyStoreError>;
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub struct PostgresApiKeyStore {
    pool: sqlx::PgPool,
}

impl PostgresApiKeyStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyStore for PostgresApiKeyStore {
    async fn tenant_for_key(&self, api_key: &str) -> Result<Option<Uuid>, ApiKeyStoreError> {
        let key_hash = hash_api_key(api_key);
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT tenant_id FROM api_keys WHERE key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;
        Ok(row.map(|(tenant_id,)| tenant_id))
    }
}
