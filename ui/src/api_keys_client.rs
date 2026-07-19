#[path = "api_keys_client_test.rs"]
#[cfg(test)]
pub(crate) mod api_keys_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ApiKeySummary {
    pub id: Uuid,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum ApiKeysClientError {
    #[error("ingestion gateway unreachable: {0}")]
    Unreachable(String),
    #[error("ingestion gateway rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Manages connector/agent API keys via Ingestion Gateway's admin endpoints — same
/// direct-call trust boundary as `AgentsClient`/`TriggersClient` (no gateway sits in front of
/// Ingestion Gateway's own admin API; it trusts `X-Tenant-Id` from Console UI's session the
/// same way config-admin-service does).
#[async_trait]
pub trait ApiKeysClient: Send + Sync {
    async fn list_api_keys(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApiKeySummary>, ApiKeysClientError>;

    /// Creates a key and returns its plaintext value — the only time it's ever available.
    async fn create_api_key(
        &self,
        tenant_id: Uuid,
        label: &str,
    ) -> Result<String, ApiKeysClientError>;

    async fn revoke_api_key(&self, tenant_id: Uuid, id: Uuid) -> Result<(), ApiKeysClientError>;
}

pub struct HttpApiKeysClient {
    client: reqwest::Client,
    ingestion_gateway_url: String,
}

impl HttpApiKeysClient {
    pub fn new(client: reqwest::Client, ingestion_gateway_url: String) -> Self {
        Self { client, ingestion_gateway_url }
    }
}

#[async_trait]
impl ApiKeysClient for HttpApiKeysClient {
    async fn list_api_keys(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApiKeySummary>, ApiKeysClientError> {
        let response = self
            .client
            .get(format!("{}/v1/api-keys", self.ingestion_gateway_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| ApiKeysClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ApiKeysClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| ApiKeysClientError::Unreachable(e.to_string()))
    }

    async fn create_api_key(
        &self,
        tenant_id: Uuid,
        label: &str,
    ) -> Result<String, ApiKeysClientError> {
        let response = self
            .client
            .post(format!("{}/v1/api-keys", self.ingestion_gateway_url))
            .header("x-tenant-id", tenant_id.to_string())
            .json(&serde_json::json!({"label": label}))
            .send()
            .await
            .map_err(|e| ApiKeysClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ApiKeysClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct CreatedApiKeyResponse {
            api_key: String,
        }
        let body: CreatedApiKeyResponse =
            response.json().await.map_err(|e| ApiKeysClientError::Unreachable(e.to_string()))?;
        Ok(body.api_key)
    }

    async fn revoke_api_key(&self, tenant_id: Uuid, id: Uuid) -> Result<(), ApiKeysClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/api-keys/{id}", self.ingestion_gateway_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| ApiKeysClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ApiKeysClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }
}
