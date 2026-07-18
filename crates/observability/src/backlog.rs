#[path = "backlog_test.rs"]
#[cfg(test)]
pub(crate) mod backlog_test;

use crate::pipeline_queues::PIPELINE_QUEUES;
use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueueDepth {
    pub stage: String,
    pub queue_name: String,
    pub messages: u64,
}

#[derive(Debug, Error)]
pub enum BacklogError {
    #[error("rabbitmq management API unreachable: {0}")]
    Unreachable(String),
}

/// Reads per-stage pipeline backlog (spec §6 service #13: "pipeline backlog/lag visibility")
/// from RabbitMQ's management HTTP API — the ingest → normalize → analyze → act chain's queue
/// depths, one entry per `PIPELINE_QUEUES` stage (ADR-0012).
#[async_trait]
pub trait BacklogReader: Send + Sync {
    async fn queue_depths(&self) -> Result<Vec<QueueDepth>, BacklogError>;
}

pub struct RabbitMqManagementBacklogReader {
    client: reqwest::Client,
    management_url: String,
    vhost: String,
}

impl RabbitMqManagementBacklogReader {
    pub fn new(client: reqwest::Client, management_url: String, vhost: String) -> Self {
        Self { client, management_url, vhost }
    }
}

#[derive(serde::Deserialize)]
struct QueueApiResponse {
    #[serde(default)]
    messages: u64,
}

#[async_trait]
impl BacklogReader for RabbitMqManagementBacklogReader {
    async fn queue_depths(&self) -> Result<Vec<QueueDepth>, BacklogError> {
        let encoded_vhost = urlencoding_percent_encode(&self.vhost);
        let mut depths = Vec::with_capacity(PIPELINE_QUEUES.len());

        for (stage, queue_name) in PIPELINE_QUEUES {
            let url =
                format!("{}/api/queues/{}/{}", self.management_url, encoded_vhost, queue_name);
            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| BacklogError::Unreachable(e.to_string()))?;

            let messages = if response.status() == reqwest::StatusCode::NOT_FOUND {
                // Queue not yet declared (its consumer hasn't started) — zero backlog, not an
                // error, since a service that has never run has nothing queued for it.
                0
            } else if response.status().is_success() {
                response
                    .json::<QueueApiResponse>()
                    .await
                    .map_err(|e| BacklogError::Unreachable(e.to_string()))?
                    .messages
            } else {
                return Err(BacklogError::Unreachable(format!(
                    "unexpected status {} from {}",
                    response.status(),
                    url
                )));
            };

            depths.push(QueueDepth {
                stage: stage.to_string(),
                queue_name: queue_name.to_string(),
                messages,
            });
        }

        Ok(depths)
    }
}

/// Minimal percent-encoding for the vhost path segment — RabbitMQ's default vhost is `/`,
/// which must be encoded as `%2F` in the URL path (not left as a literal path separator).
fn urlencoding_percent_encode(value: &str) -> String {
    value.replace('/', "%2F")
}
