#[path = "analysis_config_client_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_config_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct AnalysisConfigView {
    pub prompt: String,
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

    async fn put_analysis_config(
        &self,
        tenant_id: Uuid,
        role: Role,
        prompt: &str,
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
        prompt: &str,
    ) -> Result<AnalysisConfigView, AnalysisConfigClientError> {
        let response = self
            .client
            .put(format!("{}/v1/analysis-config", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&serde_json::json!({"prompt": prompt}))
            .send()
            .await
            .map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisConfigClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))
    }
}
