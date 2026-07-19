#[path = "ingestion_client_test.rs"]
#[cfg(test)]
pub(crate) mod ingestion_client_test;

use async_trait::async_trait;
use common::RawRecord;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestionClientError {
    #[error("ingestion gateway unreachable: {0}")]
    Unreachable(String),
    #[error("ingestion gateway rejected the record: HTTP {0}")]
    Rejected(u16),
}

/// Posts a polled `RawRecord` to Ingestion Gateway's `POST /v1/ingest` (ADR-0013) — every
/// connector's only path to the platform, authenticated the same way any external agent is
/// (API key), never a direct call into Ingestion Service.
#[async_trait]
pub trait IngestionClient: Send + Sync {
    async fn ingest(&self, record: &RawRecord) -> Result<(), IngestionClientError>;
}

pub struct HttpIngestionClient {
    client: reqwest::Client,
    ingestion_gateway_url: String,
    api_key: String,
}

impl HttpIngestionClient {
    pub fn new(client: reqwest::Client, ingestion_gateway_url: String, api_key: String) -> Self {
        Self { client, ingestion_gateway_url, api_key }
    }
}

#[async_trait]
impl IngestionClient for HttpIngestionClient {
    async fn ingest(&self, record: &RawRecord) -> Result<(), IngestionClientError> {
        // tenant_id is intentionally omitted: Ingestion Gateway derives it from the
        // authenticated API key, never from the request body (spec §8 tenant isolation) — any
        // value sent here would be overwritten server-side anyway.
        let response = self
            .client
            .post(format!("{}/v1/ingest", self.ingestion_gateway_url))
            .header("x-api-key", &self.api_key)
            .json(&serde_json::json!({
                "connector_id": record.connector_id,
                "source_type": record.source_type,
                "raw_payload": record.raw_payload,
                "occurred_at": record.occurred_at,
                "external_id": record.external_id,
            }))
            .send()
            .await
            .map_err(|e| IngestionClientError::Unreachable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(IngestionClientError::Rejected(response.status().as_u16()))
        }
    }
}
