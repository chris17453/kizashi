#[path = "analysis_client_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_client_test;

use async_trait::async_trait;
use common::RawRecord;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("AI/ML backend unreachable: {0}")]
    Unreachable(String),
    #[error("AI/ML backend rejected the batch: HTTP {0}")]
    Rejected(u16),
    #[error("AI/ML backend returned {got} results for a batch of {expected}")]
    ResultCountMismatch { expected: usize, got: usize },
}

/// Calls Azure AI Foundry/ML for a tenant-homogeneous batch of records (ADR-0004: analysis is
/// invoked in micro-batches, never mixing tenants in one call). Returns exactly one analysis
/// result per input record, in the same order, so callers can zip results back onto records
/// without needing a correlation id round-trip.
#[async_trait]
pub trait AnalysisClient: Send + Sync {
    async fn analyze_batch(
        &self,
        tenant_id: Uuid,
        records: &[RawRecord],
    ) -> Result<Vec<serde_json::Value>, AnalysisError>;
}

pub struct FoundryAnalysisClient {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl FoundryAnalysisClient {
    pub fn new(client: reqwest::Client, endpoint: String, api_key: String) -> Self {
        Self { client, endpoint, api_key }
    }
}

#[async_trait]
impl AnalysisClient for FoundryAnalysisClient {
    async fn analyze_batch(
        &self,
        tenant_id: Uuid,
        records: &[RawRecord],
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        let payloads: Vec<&serde_json::Value> = records
            .iter()
            .map(|r| r.normalized_payload.as_ref().unwrap_or(&r.raw_payload))
            .collect();

        let response = self
            .client
            .post(&self.endpoint)
            .header("api-key", &self.api_key)
            .json(&serde_json::json!({"tenant_id": tenant_id, "inputs": payloads}))
            .send()
            .await
            .map_err(|e| AnalysisError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct FoundryResponse {
            results: Vec<serde_json::Value>,
        }
        let body: FoundryResponse =
            response.json().await.map_err(|e| AnalysisError::Unreachable(e.to_string()))?;

        if body.results.len() != records.len() {
            return Err(AnalysisError::ResultCountMismatch {
                expected: records.len(),
                got: body.results.len(),
            });
        }
        Ok(body.results)
    }
}
