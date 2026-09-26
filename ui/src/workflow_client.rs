#[cfg(test)]
#[path = "workflow_client_test.rs"]
mod workflow_client_test;

use async_trait::async_trait;
use common::{Role, WorkflowCase, WorkflowCaseStatus};
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

static CLIENT: OnceLock<Arc<dyn WorkflowClient>> = OnceLock::new();
pub fn initialize(client: Arc<dyn WorkflowClient>) {
    let _ = CLIENT.set(client);
}
pub fn global() -> Option<Arc<dyn WorkflowClient>> {
    CLIENT.get().cloned()
}

#[derive(Debug, Error)]
pub enum WorkflowClientError {
    #[error("workflow runtime unavailable: {0}")]
    Unavailable(String),
    #[error("workflow runtime rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[async_trait]
pub trait WorkflowClient: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<WorkflowCase>, WorkflowClientError>;
    async fn decide(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        id: Uuid,
        status: WorkflowCaseStatus,
        note: Option<String>,
    ) -> Result<WorkflowCase, WorkflowClientError>;
}
pub struct HttpWorkflowClient {
    client: reqwest::Client,
    url: String,
}
impl HttpWorkflowClient {
    pub fn new(client: reqwest::Client, url: String) -> Self {
        Self { client, url }
    }
    async fn response<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, WorkflowClientError> {
        if !response.status().is_success() {
            return Err(WorkflowClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|error| WorkflowClientError::Unavailable(error.to_string()))
    }
}
#[async_trait]
impl WorkflowClient for HttpWorkflowClient {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<WorkflowCase>, WorkflowClientError> {
        let response = self
            .client
            .get(format!("{}/v1/workflow-cases", self.url.trim_end_matches('/')))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|error| WorkflowClientError::Unavailable(error.to_string()))?;
        Self::response(response).await
    }
    async fn decide(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        id: Uuid,
        status: WorkflowCaseStatus,
        note: Option<String>,
    ) -> Result<WorkflowCase, WorkflowClientError> {
        let response = self
            .client
            .post(format!("{}/v1/workflow-cases/{id}/decide", self.url.trim_end_matches('/')))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&serde_json::json!({"status":status,"actor":actor,"note":note}))
            .send()
            .await
            .map_err(|error| WorkflowClientError::Unavailable(error.to_string()))?;
        Self::response(response).await
    }
}
