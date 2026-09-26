#[cfg(test)]
#[path = "ontology_projection_client_test.rs"]
mod ontology_projection_client_test;

use crate::ReconciliationProjection;
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ProjectionClientError {
    #[error("ontology projection request failed: {0}")]
    Transport(String),
    #[error("ontology projection was rejected with HTTP {0}")]
    Rejected(reqwest::StatusCode),
}

#[async_trait]
pub trait ProjectionClient: Send + Sync {
    async fn reconcile(
        &self,
        tenant_id: Uuid,
        projection: &ReconciliationProjection,
    ) -> Result<(), ProjectionClientError>;
}

pub struct HttpProjectionClient {
    client: reqwest::Client,
    base_url: String,
    internal_secret: String,
}
impl HttpProjectionClient {
    pub fn new(client: reqwest::Client, base_url: String, internal_secret: String) -> Self {
        Self { client, base_url, internal_secret }
    }
}

#[async_trait]
impl ProjectionClient for HttpProjectionClient {
    async fn reconcile(
        &self,
        tenant_id: Uuid,
        projection: &ReconciliationProjection,
    ) -> Result<(), ProjectionClientError> {
        let url = format!(
            "{}/api/ontology/objects/{}",
            self.base_url.trim_end_matches('/'),
            projection.object_id
        );
        let response = self
            .client
            .get(&url)
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|error| ProjectionClientError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(ProjectionClientError::Rejected(response.status()));
        }
        let existing: serde_json::Value = response
            .json()
            .await
            .map_err(|error| ProjectionClientError::Transport(error.to_string()))?;
        let mut properties =
            existing.get("properties").cloned().unwrap_or_else(|| serde_json::json!({}));
        properties
            .as_object_mut()
            .ok_or_else(|| {
                ProjectionClientError::Transport(
                    "ontology object properties were not an object".to_string(),
                )
            })?
            .extend(projection.properties.as_object().unwrap().clone());
        let source_lineage = serde_json::json!({"authoritative_source_version":projection.source_version,"confirmation_evidence":projection.evidence});
        let response = self.client.post(&url).header("x-tenant-id", tenant_id.to_string()).header("x-role", "operator").header("x-actor", "pipeline-runtime").header("x-internal-secret", &self.internal_secret).json(&serde_json::json!({"object_type_id":projection.object_type_id,"properties":properties,"source_lineage":source_lineage})).send().await.map_err(|error| ProjectionClientError::Transport(error.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ProjectionClientError::Rejected(response.status()))
        }
    }
}
