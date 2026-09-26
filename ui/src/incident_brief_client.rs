#[path = "incident_brief_client_test.rs"]
#[cfg(test)]
pub(crate) mod incident_brief_client_test;

use async_trait::async_trait;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

static CLIENT: OnceLock<Arc<dyn IncidentBriefClient>> = OnceLock::new();

pub fn initialize(client: Arc<dyn IncidentBriefClient>) {
    let _ = CLIENT.set(client);
}

pub fn global() -> Option<Arc<dyn IncidentBriefClient>> {
    CLIENT.get().cloned()
}

#[derive(Debug, Error)]
pub enum IncidentBriefClientError {
    #[error("analysis service unreachable: {0}")]
    Unreachable(String),
    #[error("analysis service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[async_trait]
pub trait IncidentBriefClient: Send + Sync {
    async fn generate(
        &self,
        tenant_id: Uuid,
        evidence: serde_json::Value,
    ) -> Result<String, IncidentBriefClientError>;
}

pub struct HttpIncidentBriefClient {
    client: reqwest::Client,
    analysis_service_url: String,
}

impl HttpIncidentBriefClient {
    pub fn new(client: reqwest::Client, analysis_service_url: String) -> Self {
        Self { client, analysis_service_url }
    }
}

#[derive(serde::Deserialize)]
struct BriefResponse {
    summary: String,
}

#[async_trait]
impl IncidentBriefClient for HttpIncidentBriefClient {
    async fn generate(
        &self,
        tenant_id: Uuid,
        evidence: serde_json::Value,
    ) -> Result<String, IncidentBriefClientError> {
        let response = self
            .client
            .post(format!("{}/v1/incident-brief", self.analysis_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .json(&serde_json::json!({ "evidence": evidence }))
            .send()
            .await
            .map_err(|error| IncidentBriefClientError::Unreachable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(IncidentBriefClientError::Rejected(response.status().as_u16()));
        }
        response
            .json::<BriefResponse>()
            .await
            .map(|body| body.summary)
            .map_err(|error| IncidentBriefClientError::Unreachable(error.to_string()))
    }
}
