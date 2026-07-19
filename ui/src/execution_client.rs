#[path = "execution_client_test.rs"]
#[cfg(test)]
pub(crate) mod execution_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ActionExecutionSummary {
    pub id: Uuid,
    pub trigger_id: Uuid,
    pub event_id: Uuid,
    pub action_type: String,
    pub status: String,
    pub executed_at: DateTime<Utc>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum ExecutionClientError {
    #[error("action executor unreachable: {0}")]
    Unreachable(String),
    #[error("action executor rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads action executions from Action Executor's own admin endpoint (its first read/query
/// surface, ADR-0017 follow-up) — direct-call trust boundary like `TriggersClient`/
/// `AgentsClient` (no gateway sits in front of Action Executor either).
#[async_trait]
pub trait ExecutionClient: Send + Sync {
    async fn list_executions_for_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecutionSummary>, ExecutionClientError>;
}

pub struct HttpExecutionClient {
    client: reqwest::Client,
    action_executor_url: String,
}

impl HttpExecutionClient {
    pub fn new(client: reqwest::Client, action_executor_url: String) -> Self {
        Self { client, action_executor_url }
    }
}

#[async_trait]
impl ExecutionClient for HttpExecutionClient {
    async fn list_executions_for_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecutionSummary>, ExecutionClientError> {
        let response = self
            .client
            .get(format!("{}/v1/action-executions", self.action_executor_url))
            .query(&[("event_id", event_id.to_string())])
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ExecutionClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| ExecutionClientError::Unreachable(e.to_string()))
    }
}
