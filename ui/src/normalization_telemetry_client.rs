#[path = "normalization_telemetry_client_test.rs"]
#[cfg(test)]
pub(crate) mod normalization_telemetry_client_test;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

static CLIENT: OnceLock<Arc<dyn NormalizationTelemetryClient>> = OnceLock::new();

pub fn initialize(client: Arc<dyn NormalizationTelemetryClient>) {
    let _ = CLIENT.set(client);
}

pub fn global() -> Option<Arc<dyn NormalizationTelemetryClient>> {
    CLIENT.get().cloned()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DedupSummary {
    pub fingerprint_count: i64,
    pub active_duplicate_count: i64,
    pub suppressed_count: i64,
}

#[derive(Debug, Error)]
pub enum NormalizationTelemetryClientError {
    #[error("normalization service unreachable: {0}")]
    Unreachable(String),
    #[error("normalization service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[async_trait]
pub trait NormalizationTelemetryClient: Send + Sync {
    async fn dedup_summary(
        &self,
        tenant_id: Uuid,
    ) -> Result<DedupSummary, NormalizationTelemetryClientError>;
}

pub struct HttpNormalizationTelemetryClient {
    client: reqwest::Client,
    normalization_service_url: String,
}

impl HttpNormalizationTelemetryClient {
    pub fn new(client: reqwest::Client, normalization_service_url: String) -> Self {
        Self { client, normalization_service_url }
    }
}

#[async_trait]
impl NormalizationTelemetryClient for HttpNormalizationTelemetryClient {
    async fn dedup_summary(
        &self,
        tenant_id: Uuid,
    ) -> Result<DedupSummary, NormalizationTelemetryClientError> {
        let response = self
            .client
            .get(format!("{}/v1/dedup/summary", self.normalization_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|error| NormalizationTelemetryClientError::Unreachable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(NormalizationTelemetryClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|error| NormalizationTelemetryClientError::Unreachable(error.to_string()))
    }
}
