#[path = "tenant_repository_test.rs"]
#[cfg(test)]
pub(crate) mod tenant_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use crate::oidc_client::OidcProviderConfig;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use base64::Engine;
use rand::RngCore;
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TenantRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantOidcProviderSummary {
    pub provider: String,
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub redirect_url: String,
    pub has_client_secret: bool,
}

#[derive(Clone)]
pub struct OidcCredentialCipher {
    key: [u8; 32],
}

impl OidcCredentialCipher {
    pub fn from_base64(value: &str) -> Result<Self, TenantRepositoryError> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(value).map_err(|e| {
            TenantRepositoryError::Backend(format!("invalid OIDC encryption key: {e}"))
        })?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| {
            TenantRepositoryError::Backend(
                "OIDC encryption key must decode to 32 bytes".to_string(),
            )
        })?;
        Ok(Self { key })
    }

    pub fn from_env() -> Result<Option<Self>, TenantRepositoryError> {
        match std::env::var("OIDC_CREDENTIALS_ENCRYPTION_KEY") {
            Ok(value) if !value.trim().is_empty() => Self::from_base64(value.trim()).map(Some),
            _ => Ok(None),
        }
    }

    fn encrypt(&self, secret: &str) -> Result<(Vec<u8>, Vec<u8>), TenantRepositoryError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| TenantRepositoryError::Backend(format!("OIDC cipher init failed: {e}")))?;
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ciphertext =
            cipher.encrypt(Nonce::from_slice(&nonce), secret.as_bytes()).map_err(|_| {
                TenantRepositoryError::Backend("OIDC secret encryption failed".to_string())
            })?;
        Ok((ciphertext, nonce.to_vec()))
    }

    fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String, TenantRepositoryError> {
        if nonce.len() != 12 {
            return Err(TenantRepositoryError::Backend("invalid OIDC secret nonce".to_string()));
        }
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| TenantRepositoryError::Backend(format!("OIDC cipher init failed: {e}")))?;
        let plaintext = cipher.decrypt(Nonce::from_slice(nonce), ciphertext).map_err(|_| {
            TenantRepositoryError::Backend("OIDC secret decryption failed".to_string())
        })?;
        String::from_utf8(plaintext).map_err(|_| {
            TenantRepositoryError::Backend("OIDC secret is not valid UTF-8".to_string())
        })
    }
}

/// Resolves a human-typed workspace name to its `tenant_id` (spec §8 tenants are otherwise
/// bare UUIDs everywhere else in the system — this is the one place a person, not a service,
/// has to identify one, so it needs a name people can actually type).
#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn id_for_name(&self, name: &str) -> Result<Option<Uuid>, TenantRepositoryError>;

    async fn mfa_required(&self, tenant_id: Uuid) -> Result<bool, TenantRepositoryError>;

    async fn set_mfa_required(
        &self,
        tenant_id: Uuid,
        required: bool,
        actor: &str,
    ) -> Result<(), TenantRepositoryError>;

    async fn oidc_provider(&self, tenant_id: Uuid)
        -> Result<Option<String>, TenantRepositoryError>;

    async fn set_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: Option<&str>,
        actor: &str,
    ) -> Result<(), TenantRepositoryError>;

    async fn list_tenant_oidc_providers(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<TenantOidcProviderSummary>, TenantRepositoryError> {
        Ok(Vec::new())
    }

    async fn tenant_oidc_config(
        &self,
        _tenant_id: Uuid,
        _provider: &str,
    ) -> Result<Option<OidcProviderConfig>, TenantRepositoryError> {
        Ok(None)
    }

    async fn upsert_tenant_oidc_provider(
        &self,
        _tenant_id: Uuid,
        _provider: &str,
        _config: &OidcProviderConfig,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        Err(TenantRepositoryError::Backend(
            "tenant OIDC credential storage is not configured".to_string(),
        ))
    }

    async fn delete_tenant_oidc_provider(
        &self,
        _tenant_id: Uuid,
        _provider: &str,
        _actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        Err(TenantRepositoryError::Backend(
            "tenant OIDC credential storage is not configured".to_string(),
        ))
    }
}

pub struct PostgresTenantRepository {
    pool: sqlx::PgPool,
    oidc_cipher: Option<OidcCredentialCipher>,
}

impl PostgresTenantRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        let oidc_cipher = match OidcCredentialCipher::from_env() {
            Ok(cipher) => cipher,
            Err(error) => {
                tracing::error!(error = %error, "tenant OIDC credentials disabled");
                None
            }
        };
        Self { pool, oidc_cipher }
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

    async fn mfa_required(&self, tenant_id: Uuid) -> Result<bool, TenantRepositoryError> {
        sqlx::query_scalar("SELECT mfa_required FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| TenantRepositoryError::Backend(e.to_string()))
    }

    async fn set_mfa_required(
        &self,
        tenant_id: Uuid,
        required: bool,
        actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        let before: bool = sqlx::query_scalar("SELECT mfa_required FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        sqlx::query("UPDATE tenants SET mfa_required = $2 WHERE id = $1")
            .bind(tenant_id)
            .bind(required)
            .execute(&mut *tx)
            .await
            .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "tenant_security_policy".to_string(),
                entity_id: tenant_id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: Some(serde_json::json!({"mfa_required": before})),
                after: serde_json::json!({"mfa_required": required}),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn oidc_provider(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<String>, TenantRepositoryError> {
        sqlx::query_scalar("SELECT oidc_provider FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| TenantRepositoryError::Backend(e.to_string()))
    }

    async fn set_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: Option<&str>,
        actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        let before: Option<String> =
            sqlx::query_scalar("SELECT oidc_provider FROM tenants WHERE id = $1")
                .bind(tenant_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        sqlx::query("UPDATE tenants SET oidc_provider = $2 WHERE id = $1")
            .bind(tenant_id)
            .bind(provider)
            .execute(&mut *tx)
            .await
            .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "tenant_security_policy".to_string(),
                entity_id: tenant_id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: Some(serde_json::json!({"oidc_provider": before})),
                after: serde_json::json!({"oidc_provider": provider}),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_tenant_oidc_providers(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantOidcProviderSummary>, TenantRepositoryError> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String)>(
            "SELECT provider, client_id, auth_url, token_url, userinfo_url, redirect_url
             FROM tenant_oidc_providers WHERE tenant_id = $1 ORDER BY provider",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(provider, client_id, auth_url, token_url, userinfo_url, redirect_url)| {
                TenantOidcProviderSummary {
                    provider,
                    client_id,
                    auth_url,
                    token_url,
                    userinfo_url,
                    redirect_url,
                    has_client_secret: true,
                }
            })
            .collect())
    }

    async fn tenant_oidc_config(
        &self,
        tenant_id: Uuid,
        provider: &str,
    ) -> Result<Option<OidcProviderConfig>, TenantRepositoryError> {
        let row = sqlx::query_as::<_, (String, Vec<u8>, Vec<u8>, String, String, String, String)>(
            "SELECT client_id, client_secret_ciphertext, client_secret_nonce, auth_url, token_url,
                    userinfo_url, redirect_url
             FROM tenant_oidc_providers WHERE tenant_id = $1 AND provider = $2",
        )
        .bind(tenant_id)
        .bind(provider)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        let Some((client_id, ciphertext, nonce, auth_url, token_url, userinfo_url, redirect_url)) =
            row
        else {
            return Ok(None);
        };
        let cipher = self.oidc_cipher.as_ref().ok_or_else(|| {
            TenantRepositoryError::Backend(
                "OIDC_CREDENTIALS_ENCRYPTION_KEY is not configured".to_string(),
            )
        })?;
        Ok(Some(OidcProviderConfig {
            client_id,
            client_secret: cipher.decrypt(&ciphertext, &nonce)?,
            auth_url,
            token_url,
            userinfo_url,
            redirect_url,
        }))
    }

    async fn upsert_tenant_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: &str,
        config: &OidcProviderConfig,
        actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        let cipher = self.oidc_cipher.as_ref().ok_or_else(|| {
            TenantRepositoryError::Backend(
                "OIDC_CREDENTIALS_ENCRYPTION_KEY is not configured".to_string(),
            )
        })?;
        let (ciphertext, nonce) = cipher.encrypt(&config.client_secret)?;
        let mut tx =
            self.pool.begin().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        sqlx::query(
            "INSERT INTO tenant_oidc_providers
             (tenant_id, provider, client_id, client_secret_ciphertext, client_secret_nonce,
              auth_url, token_url, userinfo_url, redirect_url)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (tenant_id, provider) DO UPDATE SET
              client_id=EXCLUDED.client_id, client_secret_ciphertext=EXCLUDED.client_secret_ciphertext,
              client_secret_nonce=EXCLUDED.client_secret_nonce, auth_url=EXCLUDED.auth_url,
              token_url=EXCLUDED.token_url, userinfo_url=EXCLUDED.userinfo_url,
              redirect_url=EXCLUDED.redirect_url, updated_at=now()",
        )
        .bind(tenant_id).bind(provider).bind(&config.client_id).bind(ciphertext).bind(nonce)
        .bind(&config.auth_url).bind(&config.token_url).bind(&config.userinfo_url).bind(&config.redirect_url)
        .execute(&mut *tx).await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "tenant_oidc_provider".to_string(),
                entity_id: tenant_id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: None,
                after: serde_json::json!({"provider": provider, "client_id": config.client_id,
                "auth_url": config.auth_url, "token_url": config.token_url,
                "userinfo_url": config.userinfo_url, "redirect_url": config.redirect_url,
                "client_secret": "[redacted]"}),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn delete_tenant_oidc_provider(
        &self,
        tenant_id: Uuid,
        provider: &str,
        actor: &str,
    ) -> Result<(), TenantRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        let result =
            sqlx::query("DELETE FROM tenant_oidc_providers WHERE tenant_id = $1 AND provider = $2")
                .bind(tenant_id)
                .bind(provider)
                .execute(&mut *tx)
                .await
                .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(TenantRepositoryError::Backend(
                "tenant OIDC provider not found".to_string(),
            ));
        }
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "tenant_oidc_provider".to_string(),
                entity_id: tenant_id,
                change_type: ChangeType::Deleted,
                actor: actor.to_string(),
                before: Some(serde_json::json!({"provider": provider})),
                after: serde_json::json!({}),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| TenantRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
