#[cfg(test)]
#[path = "command_adapter_test.rs"]
mod command_adapter_test;

use async_trait::async_trait;
use thiserror::Error;

/// The minimum governed command shape. The optimistic version comes from the authoritative
/// system's last observed representation, never a Kizashi projection version.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDispatch {
    pub endpoint: String,
    pub idempotency_key: String,
    pub expected_version: String,
    pub payload: serde_json::Value,
}
impl CommandDispatch {
    pub fn is_valid(&self) -> bool {
        self.endpoint.starts_with("https://")
            && !self.idempotency_key.trim().is_empty()
            && !self.expected_version.trim().is_empty()
    }
}

#[derive(Debug, Error)]
pub enum CommandDispatchError {
    #[error("command endpoint, idempotency key, and expected version are required")]
    InvalidDispatch,
    #[error("external command request failed: {0}")]
    Transport(String),
    #[error("external command was rejected with HTTP status {0}")]
    Rejected(reqwest::StatusCode),
}

#[async_trait]
pub trait CommandAdapter: Send + Sync {
    async fn dispatch(
        &self,
        command: &CommandDispatch,
    ) -> Result<serde_json::Value, CommandDispatchError>;
}

pub struct HttpCommandAdapter {
    client: reqwest::Client,
}
impl HttpCommandAdapter {
    /// `egress_proxy_url` follows ADR-0021: customer-controlled endpoints can be forced
    /// through Egress Gateway without coupling this runtime to a specific adapter vendor.
    pub fn new(
        egress_proxy_url: Option<&str>,
        tenant_id: uuid::Uuid,
    ) -> Result<Self, CommandDispatchError> {
        common::build_outbound_client(egress_proxy_url, tenant_id, "pipeline-runtime")
            .map(|client| Self { client })
            .map_err(|error| CommandDispatchError::Transport(error.to_string()))
    }
}

#[async_trait]
impl CommandAdapter for HttpCommandAdapter {
    async fn dispatch(
        &self,
        command: &CommandDispatch,
    ) -> Result<serde_json::Value, CommandDispatchError> {
        if !command.is_valid() {
            return Err(CommandDispatchError::InvalidDispatch);
        }
        let response = self
            .client
            .post(&command.endpoint)
            .header("Idempotency-Key", &command.idempotency_key)
            .header(reqwest::header::IF_MATCH, &command.expected_version)
            .json(&command.payload)
            .send()
            .await
            .map_err(|error| CommandDispatchError::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(CommandDispatchError::Rejected(status));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| CommandDispatchError::Transport(error.to_string()))?;
        if body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&body).map_err(|error| {
            CommandDispatchError::Transport(format!("command response was not JSON: {error}"))
        })
    }
}
