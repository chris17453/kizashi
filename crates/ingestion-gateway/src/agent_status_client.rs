#[path = "agent_status_client_test.rs"]
#[cfg(test)]
pub(crate) mod agent_status_client_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentStatusError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
}

/// Whether ingestion should be accepted for a given `connector_id`. `Unregistered` is the
/// permissive default — most connectors have no registered `Agent` row at all today, and this
/// must never break ingestion for them. Only an explicitly-registered-and-disabled agent
/// blocks ingestion; this is deliberately the one place an operator's "Enabled: no" toggle in
/// the Console UI actually does something, closing a real gap (previously the flag was stored
/// but never checked anywhere).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    #[default]
    Unregistered,
    Enabled,
    Disabled,
}

#[async_trait]
pub trait AgentStatusClient: Send + Sync {
    async fn status_for(
        &self,
        tenant_id: Uuid,
        connector_id: &str,
    ) -> Result<AgentStatus, AgentStatusError>;
}

pub struct HttpAgentStatusClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpAgentStatusClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl AgentStatusClient for HttpAgentStatusClient {
    async fn status_for(
        &self,
        tenant_id: Uuid,
        connector_id: &str,
    ) -> Result<AgentStatus, AgentStatusError> {
        let response = self
            .client
            .get(format!("{}/v1/agents/by-name/{connector_id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| AgentStatusError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(AgentStatus::Unregistered);
        }
        if !response.status().is_success() {
            return Err(AgentStatusError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct AgentBody {
            enabled: bool,
        }
        let body: AgentBody =
            response.json().await.map_err(|e| AgentStatusError::Unreachable(e.to_string()))?;
        Ok(if body.enabled { AgentStatus::Enabled } else { AgentStatus::Disabled })
    }
}
