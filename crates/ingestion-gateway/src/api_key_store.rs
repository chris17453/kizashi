#[path = "api_key_store_test.rs"]
#[cfg(test)]
pub(crate) mod api_key_store_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApiKeyStoreError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ApiKeySummary {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Resolves an API key presented by a connector/agent to the tenant it authenticates as
/// (spec §8: "gateway layer: auth context scopes all downstream queries"). Keys are stored
/// and looked up by their SHA-256 hash — the plaintext key is never persisted (CLAUDE.md §5),
/// only returned once, at `create` time, to the caller.
#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    async fn tenant_for_key(&self, api_key: &str) -> Result<Option<Uuid>, ApiKeyStoreError>;

    /// Creates a new key for `tenant_id`, writes a `Created` audit row (with `actor` set to the
    /// real user performing the action, not the tenant) in the same transaction, and returns the
    /// summary plus the plaintext key — the only time the plaintext is ever available, since
    /// only its hash is persisted.
    async fn create(
        &self,
        tenant_id: Uuid,
        label: &str,
        actor: &str,
    ) -> Result<(ApiKeySummary, String), ApiKeyStoreError>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, ApiKeyStoreError>;

    /// Revokes a key by id, scoped to `tenant_id`, writing a `Deleted` audit row (with `actor`
    /// set to the real user performing the action, not the tenant) in the same transaction. A
    /// no-op (not an error) if the key doesn't exist or is already revoked — revocation is
    /// idempotent by design.
    async fn revoke(&self, tenant_id: Uuid, id: Uuid, actor: &str) -> Result<(), ApiKeyStoreError>;
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generates a new plaintext API key. Uses two v4 UUIDs (each backed by the `uuid` crate's
/// CSPRNG) rather than pulling in a `rand` dependency solely for this — 244 bits of combined
/// randomness is well beyond what's needed for a bearer credential.
fn generate_api_key() -> String {
    format!("kzsh_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub struct PostgresApiKeyStore {
    pool: sqlx::PgPool,
}

impl PostgresApiKeyStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type ApiKeyRow = (Uuid, Uuid, String, DateTime<Utc>, Option<DateTime<Utc>>);

fn row_to_summary(row: ApiKeyRow) -> ApiKeySummary {
    let (id, tenant_id, label, created_at, revoked_at) = row;
    ApiKeySummary { id, tenant_id, label, created_at, revoked_at }
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

    async fn create(
        &self,
        tenant_id: Uuid,
        label: &str,
        actor: &str,
    ) -> Result<(ApiKeySummary, String), ApiKeyStoreError> {
        let plaintext = generate_api_key();
        let key_hash = hash_api_key(&plaintext);
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        let mut tx =
            self.pool.begin().await.map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;

        sqlx::query(
            "INSERT INTO api_keys (id, tenant_id, key_hash, label, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&key_hash)
        .bind(label)
        .bind(created_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;

        let summary =
            ApiKeySummary { id, tenant_id, label: label.to_string(), created_at, revoked_at: None };

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "api_key".to_string(),
                entity_id: id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::json!({"label": label}),
                changed_at: created_at,
            },
        )
        .await
        .map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;
        Ok((summary, plaintext))
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<ApiKeySummary>, ApiKeyStoreError> {
        let rows: Vec<ApiKeyRow> = sqlx::query_as(
            "SELECT id, tenant_id, label, created_at, revoked_at FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_summary).collect())
    }

    async fn revoke(&self, tenant_id: Uuid, id: Uuid, actor: &str) -> Result<(), ApiKeyStoreError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;

        if result.rows_affected() > 0 {
            record_audit_entry(
                &mut tx,
                &AuditLogEntry {
                    id: Uuid::new_v4(),
                    tenant_id,
                    entity_type: "api_key".to_string(),
                    entity_id: id,
                    change_type: ChangeType::Deleted,
                    actor: actor.to_string(),
                    before: None,
                    after: serde_json::json!({"revoked": true}),
                    changed_at: Utc::now(),
                },
            )
            .await
            .map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| ApiKeyStoreError::Backend(e.to_string()))?;
        Ok(())
    }
}
