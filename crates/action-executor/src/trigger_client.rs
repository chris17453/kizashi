#[path = "trigger_client_test.rs"]
#[cfg(test)]
pub(crate) mod trigger_client_test;

use async_trait::async_trait;
use common::TriggerDefinition;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TriggerClientError {
    #[error("trigger-engine unreachable: {0}")]
    Unreachable(String),
    #[error("trigger-engine rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Resolves a TriggerDefinition by id via Trigger Engine's API (spec §2 principle 1) — Action
/// Executor never reads Trigger Engine's Postgres schema directly.
#[async_trait]
pub trait TriggerClient: Send + Sync {
    async fn get_trigger(
        &self,
        trigger_id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerClientError>;
}

pub struct HttpTriggerClient {
    client: reqwest::Client,
    trigger_engine_url: String,
}

impl HttpTriggerClient {
    pub fn new(client: reqwest::Client, trigger_engine_url: String) -> Self {
        Self { client, trigger_engine_url }
    }
}

#[async_trait]
impl TriggerClient for HttpTriggerClient {
    async fn get_trigger(
        &self,
        trigger_id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerClientError> {
        let response = self
            .client
            .get(format!("{}/v1/triggers/{}", self.trigger_engine_url, trigger_id))
            .send()
            .await
            .map_err(|e| TriggerClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(TriggerClientError::Rejected(response.status().as_u16()));
        }

        let trigger = response
            .json::<TriggerDefinition>()
            .await
            .map_err(|e| TriggerClientError::Unreachable(e.to_string()))?;
        Ok(Some(trigger))
    }
}
