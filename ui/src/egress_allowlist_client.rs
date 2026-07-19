#[path = "egress_allowlist_client_test.rs"]
#[cfg(test)]
pub(crate) mod egress_allowlist_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EgressAllowlistClientError {
    #[error("egress-gateway unreachable: {0}")]
    Unreachable(String),
    #[error("egress-gateway rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads/replaces the calling tenant's egress domain allowlist (ADR-0021) via
/// `egress-gateway`'s `GET`/`PUT /v1/allowlist` — same direct-call trust boundary as
/// `AnalysisConfigClient` (`x-tenant-id`/`x-role` headers, no gateway sits in front of
/// `egress-gateway`'s own admin API). `egress-gateway` sends `X-Tenant-Id` as a plain string
/// (not necessarily a UUID — matches its own `tenant_id_from_headers`, which never parses it),
/// so this client takes the Console UI session's `Uuid` and stringifies it, same as every
/// other client in this crate.
#[async_trait]
pub trait EgressAllowlistClient: Send + Sync {
    async fn get_allowlist(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, EgressAllowlistClientError>;

    /// Replaces the tenant's allowlist wholesale — not a per-domain add/remove API, matching
    /// the backend's own `PUT /v1/allowlist` semantics (ADR-0021: "the list is small and
    /// operator-managed, replace-the-whole-thing").
    async fn put_allowlist(
        &self,
        tenant_id: Uuid,
        role: Role,
        domains: Vec<String>,
    ) -> Result<Vec<String>, EgressAllowlistClientError>;
}

pub struct HttpEgressAllowlistClient {
    client: reqwest::Client,
    egress_gateway_url: String,
}

impl HttpEgressAllowlistClient {
    pub fn new(client: reqwest::Client, egress_gateway_url: String) -> Self {
        Self { client, egress_gateway_url }
    }
}

#[async_trait]
impl EgressAllowlistClient for HttpEgressAllowlistClient {
    async fn get_allowlist(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, EgressAllowlistClientError> {
        let response = self
            .client
            .get(format!("{}/v1/allowlist", self.egress_gateway_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| EgressAllowlistClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(EgressAllowlistClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| EgressAllowlistClientError::Unreachable(e.to_string()))
    }

    async fn put_allowlist(
        &self,
        tenant_id: Uuid,
        role: Role,
        domains: Vec<String>,
    ) -> Result<Vec<String>, EgressAllowlistClientError> {
        let response = self
            .client
            .put(format!("{}/v1/allowlist", self.egress_gateway_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&serde_json::json!({"domains": domains}))
            .send()
            .await
            .map_err(|e| EgressAllowlistClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(EgressAllowlistClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| EgressAllowlistClientError::Unreachable(e.to_string()))
    }
}
