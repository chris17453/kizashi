#[path = "analysis_config_client_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_config_client_test;

use async_trait::async_trait;
use common::{AnalysisProvider, Role};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AnalysisConfigView {
    pub prompt: String,
    #[serde(default)]
    pub provider: AnalysisProvider,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// What an operator submits through the Console UI's analysis-config form (ADR-0031). Kept
/// separate from `AnalysisConfigView` (the read shape) since a PUT never needs `updated_at`.
pub struct AnalysisConfigInput<'a> {
    pub prompt: &'a str,
    pub provider: AnalysisProvider,
    pub model: Option<&'a str>,
    pub endpoint: Option<&'a str>,
    pub api_key: Option<&'a str>,
}

#[derive(Debug, Error)]
pub enum AnalysisConfigClientError {
    #[error("config-admin-service unreachable: {0}")]
    Unreachable(String),
    #[error("config-admin-service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads/writes the calling tenant's AI analysis prompt (ADR-0019) via config-admin-service's
/// `/v1/analysis-config` — same direct-call trust boundary as `TriggersClient`
/// (`x-tenant-id`/`x-role` headers, no gateway sits in front of config-admin-service).
#[async_trait]
pub trait AnalysisConfigClient: Send + Sync {
    async fn get_analysis_config(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfigView>, AnalysisConfigClientError>;

    /// `actor` is the signed-in session's username, sent as `X-Username` so config-admin-service
    /// can record the real actor on the audit-log entry instead of just the tenant.
    async fn put_analysis_config(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        input: AnalysisConfigInput<'_>,
    ) -> Result<AnalysisConfigView, AnalysisConfigClientError>;
}

pub struct HttpAnalysisConfigClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpAnalysisConfigClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl AnalysisConfigClient for HttpAnalysisConfigClient {
    async fn get_analysis_config(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfigView>, AnalysisConfigClientError> {
        let response = self
            .client
            .get(format!("{}/v1/analysis-config", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisConfigClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))
    }

    async fn put_analysis_config(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        input: AnalysisConfigInput<'_>,
    ) -> Result<AnalysisConfigView, AnalysisConfigClientError> {
        let response = self
            .client
            .put(format!("{}/v1/analysis-config", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({
                "prompt": input.prompt,
                "provider": input.provider,
                "model": input.model,
                "endpoint": input.endpoint,
                "api_key": input.api_key,
            }))
            .send()
            .await
            .map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisConfigClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))
    }
}
