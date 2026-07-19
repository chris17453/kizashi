#[path = "agents_client_test.rs"]
#[cfg(test)]
pub(crate) mod agents_client_test;

use async_trait::async_trait;
use common::Agent;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentsClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Registers/lists/deletes Agents (a tenant's registered connector instances) via Config/Admin
/// Service, same direct-call trust boundary as `TriggersClient` (no gateway sits in front of
/// Config/Admin Service, ADR-0010). The operational convention this establishes: the agent's
/// registered `name` is what the corresponding connector's own `CONNECTOR_ID` env var must be
/// set to, so ingested records (`connector_id`) can be matched back to a registered agent for
/// status/drill-down — see `IngestionStatsClient`.
#[async_trait]
pub trait AgentsClient: Send + Sync {
    async fn list_agents(&self, tenant_id: Uuid) -> Result<Vec<Agent>, AgentsClientError>;
    async fn register_agent(
        &self,
        tenant_id: Uuid,
        connector_type: &str,
        name: &str,
        config: serde_json::Value,
    ) -> Result<Agent, AgentsClientError>;
    async fn delete_agent(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AgentsClientError>;

    /// Persists `agent` as-is via `PUT /v1/agents/:id` — used for the enable/disable toggle
    /// (flip `agent.enabled`, then call this with the rest of the fields unchanged).
    async fn update_agent(&self, agent: &Agent) -> Result<Agent, AgentsClientError>;
}

pub struct HttpAgentsClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpAgentsClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl AgentsClient for HttpAgentsClient {
    async fn list_agents(&self, tenant_id: Uuid) -> Result<Vec<Agent>, AgentsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/agents", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| AgentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AgentsClientError::Unreachable(e.to_string()))
    }

    async fn register_agent(
        &self,
        tenant_id: Uuid,
        connector_type: &str,
        name: &str,
        config: serde_json::Value,
    ) -> Result<Agent, AgentsClientError> {
        let agent = Agent::new(tenant_id, connector_type, name, config);
        let response = self
            .client
            .post(format!("{}/v1/agents", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .json(&agent)
            .send()
            .await
            .map_err(|e| AgentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AgentsClientError::Unreachable(e.to_string()))
    }

    async fn delete_agent(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AgentsClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/agents/{id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| AgentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentsClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }

    async fn update_agent(&self, agent: &Agent) -> Result<Agent, AgentsClientError> {
        let response = self
            .client
            .put(format!("{}/v1/agents/{}", self.config_admin_service_url, agent.id))
            .header("x-tenant-id", agent.tenant_id.to_string())
            .json(agent)
            .send()
            .await
            .map_err(|e| AgentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AgentsClientError::Unreachable(e.to_string()))
    }
}
