#[path = "health_client_test.rs"]
#[cfg(test)]
pub(crate) mod health_client_test;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ServiceHealthSummary {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct PlatformHealthSummary {
    pub status: String,
    pub services: Vec<ServiceHealthSummary>,
}

#[derive(Debug, Error)]
pub enum HealthClientError {
    #[error("observability service unreachable: {0}")]
    Unreachable(String),
}

/// Reads platform-wide health from Observability (spec §6, service #13) — platform health has
/// no tenant scoping (ADR-0012), so no auth header is needed here, same as calling `/healthz`
/// on any other service.
#[async_trait]
pub trait HealthClient: Send + Sync {
    async fn platform_health(&self) -> Result<PlatformHealthSummary, HealthClientError>;
}

pub struct HttpHealthClient {
    client: reqwest::Client,
    observability_url: String,
}

impl HttpHealthClient {
    pub fn new(client: reqwest::Client, observability_url: String) -> Self {
        Self { client, observability_url }
    }
}

#[async_trait]
impl HealthClient for HttpHealthClient {
    async fn platform_health(&self) -> Result<PlatformHealthSummary, HealthClientError> {
        let response = self
            .client
            .get(format!("{}/v1/health", self.observability_url))
            .send()
            .await
            .map_err(|e| HealthClientError::Unreachable(e.to_string()))?;

        // Observability's /v1/health intentionally returns 503 when any service is down
        // (ADR-0012) — that's a successful, meaningful response for this client, not an error.
        response.json().await.map_err(|e| HealthClientError::Unreachable(e.to_string()))
    }
}
