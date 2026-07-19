#[path = "backlog_client_test.rs"]
#[cfg(test)]
pub(crate) mod backlog_client_test;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct QueueDepthSummary {
    pub stage: String,
    pub queue_name: String,
    pub messages: u64,
}

#[derive(Debug, Error)]
pub enum BacklogClientError {
    #[error("observability service unreachable: {0}")]
    Unreachable(String),
}

/// Reads pipeline backlog/lag (spec §6 service #13, ADR-0012) from Observability — no tenant
/// scoping, same as `HealthClient`, since queue depth is a platform-wide signal.
#[async_trait]
pub trait BacklogClient: Send + Sync {
    async fn queue_depths(&self) -> Result<Vec<QueueDepthSummary>, BacklogClientError>;
}

pub struct HttpBacklogClient {
    client: reqwest::Client,
    observability_url: String,
}

impl HttpBacklogClient {
    pub fn new(client: reqwest::Client, observability_url: String) -> Self {
        Self { client, observability_url }
    }
}

#[async_trait]
impl BacklogClient for HttpBacklogClient {
    async fn queue_depths(&self) -> Result<Vec<QueueDepthSummary>, BacklogClientError> {
        let response = self
            .client
            .get(format!("{}/v1/backlog", self.observability_url))
            .send()
            .await
            .map_err(|e| BacklogClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BacklogClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        response.json().await.map_err(|e| BacklogClientError::Unreachable(e.to_string()))
    }
}
