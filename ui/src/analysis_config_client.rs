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
    /// Never the real secret on a GET response (RBAC audit fix, config-admin-service side) —
    /// `None` unless this is the direct response to a PUT the caller just submitted, in which
    /// case it echoes back what they just typed. Use `api_key_configured` to know whether a key
    /// exists without seeing it.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_configured: bool,
}

/// What an operator submits through the Console UI's analysis-config form (ADR-0031). Kept
/// separate from `AnalysisConfigView` (the read shape) since a PUT never needs `updated_at`.
///
/// `api_key` is tri-state, mirroring config-admin-service's `PutAnalysisConfigBody`: `None`
/// means "don't mention it, leave whatever's stored as-is" (the only sane default now that the
/// read side never hands the real key back for a form to silently re-submit), `Some(None)`
/// means "clear it", and `Some(Some(key))` means "set it to this".
pub struct AnalysisConfigInput<'a> {
    pub prompt: &'a str,
    pub provider: AnalysisProvider,
    pub model: Option<&'a str>,
    pub endpoint: Option<&'a str>,
    pub api_key: Option<Option<&'a str>>,
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
        let mut body = serde_json::json!({
            "prompt": input.prompt,
            "provider": input.provider,
            "model": input.model,
            "endpoint": input.endpoint,
        });
        // Omit `api_key` entirely when the caller isn't asking to change it — config-admin-
        // service treats a missing field as "keep the existing key" and only an explicit value
        // (including explicit `null`) as a change, see `deserialize_optional_api_key` there.
        if let Some(api_key) = input.api_key {
            body["api_key"] = serde_json::json!(api_key);
        }

        let response = self
            .client
            .put(format!("{}/v1/analysis-config", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&body)
            .send()
            .await
            .map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisConfigClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AnalysisConfigClientError::Unreachable(e.to_string()))
    }
}
