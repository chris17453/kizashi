#[path = "session_client_test.rs"]
#[cfg(test)]
pub(crate) mod session_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SessionClientError {
    #[error("query-gateway unreachable: {0}")]
    Unreachable(String),
    #[error("query-gateway rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Mints a session token for a successfully-authenticated user by calling Query Gateway's
/// internal API (ADR-0008/0009) — Auth Service never writes into `query_api_tokens` directly
/// (spec §2 principle 1). `role` (ADR-0016) is stored alongside the token so later resolution
/// of the token recovers both the tenant and the caller's permission level.
#[async_trait]
pub trait SessionClient: Send + Sync {
    async fn mint_session(
        &self,
        tenant_id: Uuid,
        role: Role,
        label: &str,
    ) -> Result<String, SessionClientError>;
}

pub struct HttpSessionClient {
    client: reqwest::Client,
    query_gateway_url: String,
    internal_secret: String,
}

impl HttpSessionClient {
    pub fn new(
        client: reqwest::Client,
        query_gateway_url: String,
        internal_secret: String,
    ) -> Self {
        Self { client, query_gateway_url, internal_secret }
    }
}

#[derive(serde::Deserialize)]
struct MintTokenResponse {
    token: String,
}

#[async_trait]
impl SessionClient for HttpSessionClient {
    async fn mint_session(
        &self,
        tenant_id: Uuid,
        role: Role,
        label: &str,
    ) -> Result<String, SessionClientError> {
        let response = self
            .client
            .post(format!("{}/internal/tokens", self.query_gateway_url))
            .header("x-internal-secret", &self.internal_secret)
            .json(&serde_json::json!({"tenant_id": tenant_id, "role": role, "label": label}))
            .send()
            .await
            .map_err(|e| SessionClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SessionClientError::Rejected(response.status().as_u16()));
        }

        let body: MintTokenResponse =
            response.json().await.map_err(|e| SessionClientError::Unreachable(e.to_string()))?;
        Ok(body.token)
    }
}
