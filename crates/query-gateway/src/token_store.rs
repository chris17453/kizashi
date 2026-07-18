#[path = "token_store_test.rs"]
#[cfg(test)]
pub(crate) mod token_store_test;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TokenStoreError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Resolves a user-facing bearer token to the tenant it authenticates as (spec §8, ADR-0008).
/// Same shape as ingestion-gateway's ApiKeyStore — tokens are stored and looked up by their
/// SHA-256 hash, never the plaintext.
#[async_trait]
pub trait TokenStore: Send + Sync {
    async fn tenant_for_token(&self, token: &str) -> Result<Option<Uuid>, TokenStoreError>;

    /// Mints a new session token for `tenant_id`, storing only its hash, and returns the
    /// plaintext once — this is the only time it is ever available. Auth Service calls this
    /// after a successful login (ADR-0009) rather than writing into this table directly
    /// (spec §2 principle 1).
    async fn mint_token(&self, tenant_id: Uuid, label: &str) -> Result<String, TokenStoreError>;
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub struct PostgresTokenStore {
    pool: sqlx::PgPool,
}

impl PostgresTokenStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TokenStore for PostgresTokenStore {
    async fn tenant_for_token(&self, token: &str) -> Result<Option<Uuid>, TokenStoreError> {
        let token_hash = hash_token(token);
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT tenant_id FROM query_api_tokens WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| TokenStoreError::Backend(e.to_string()))?;
        Ok(row.map(|(tenant_id,)| tenant_id))
    }

    async fn mint_token(&self, tenant_id: Uuid, label: &str) -> Result<String, TokenStoreError> {
        let token = generate_token();
        let token_hash = hash_token(&token);
        sqlx::query(
            "INSERT INTO query_api_tokens (id, tenant_id, token_hash, label, created_at) VALUES ($1, $2, $3, $4, now())",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(&token_hash)
        .bind(label)
        .execute(&self.pool)
        .await
        .map_err(|e| TokenStoreError::Backend(e.to_string()))?;
        Ok(token)
    }
}

/// 256 bits of CSPRNG entropy (two v4 UUIDs concatenated) — deliberately not derived from any
/// guessable value, since this is the plaintext bearer credential handed back to the caller.
fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
