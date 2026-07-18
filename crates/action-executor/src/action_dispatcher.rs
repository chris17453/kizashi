#[path = "action_dispatcher_test.rs"]
#[cfg(test)]
pub(crate) mod action_dispatcher_test;

use async_trait::async_trait;
use common::{ActionRef, Event};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("action config missing required `url` field")]
    MissingUrl,
    #[error("target unreachable: {0}")]
    Unreachable(String),
    #[error("target rejected the action: HTTP {0}")]
    Rejected(u16),
}

/// Executes one action for a firing event. v1 uses a single HTTP-POST dispatch model for every
/// ActionType (ADR-0007) — genuinely functional against any webhook-shaped endpoint (Teams
/// incoming webhooks, Slack, Zapier/n8n relays, most ticketing/email HTTP APIs), not a stub.
#[async_trait]
pub trait ActionDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError>;
}

pub struct HttpActionDispatcher {
    client: reqwest::Client,
}

impl HttpActionDispatcher {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ActionDispatcher for HttpActionDispatcher {
    async fn dispatch(
        &self,
        action: &ActionRef,
        event: &Event,
    ) -> Result<serde_json::Value, DispatchError> {
        let url =
            action.config.get("url").and_then(|v| v.as_str()).ok_or(DispatchError::MissingUrl)?;

        let body = serde_json::json!({
            "action_type": action.action_type,
            "action_config": action.config,
            "event": event,
        });

        let response = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DispatchError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DispatchError::Rejected(response.status().as_u16()));
        }

        let status = response.status().as_u16();
        Ok(serde_json::json!({"http_status": status}))
    }
}
