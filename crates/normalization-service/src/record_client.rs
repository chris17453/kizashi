#[path = "record_client_test.rs"]
#[cfg(test)]
pub(crate) mod record_client_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RecordClientError {
    #[error("ingestion service unreachable: {0}")]
    Unreachable(String),
    #[error("ingestion service rejected the update: HTTP {0}")]
    Rejected(u16),
}

/// Writes a computed `normalized_payload` back onto a RawRecord. Normalization Service never
/// touches Ingestion Service's Postgres table directly (spec §2 principle 1) — this is an HTTP
/// client for the `PATCH /v1/records/:id/normalized` endpoint Ingestion Service exposes for
/// exactly this purpose.
#[async_trait]
pub trait RecordClient: Send + Sync {
    async fn update_normalized_payload(
        &self,
        record_id: Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<(), RecordClientError>;
}

pub struct HttpRecordClient {
    client: reqwest::Client,
    ingestion_service_url: String,
}

impl HttpRecordClient {
    pub fn new(client: reqwest::Client, ingestion_service_url: String) -> Self {
        Self { client, ingestion_service_url }
    }
}

#[async_trait]
impl RecordClient for HttpRecordClient {
    async fn update_normalized_payload(
        &self,
        record_id: Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<(), RecordClientError> {
        let response = self
            .client
            .patch(format!("{}/v1/records/{}/normalized", self.ingestion_service_url, record_id))
            .json(&serde_json::json!({"normalized_payload": normalized_payload}))
            .send()
            .await
            .map_err(|e| RecordClientError::Unreachable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(RecordClientError::Rejected(response.status().as_u16()))
        }
    }
}
