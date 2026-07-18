#[path = "ingestion_stats_client_test.rs"]
#[cfg(test)]
pub(crate) mod ingestion_stats_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ConnectorStatSummary {
    pub connector_id: String,
    pub record_count: i64,
    pub last_ingested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct RecordSummary {
    pub id: Uuid,
    pub source_type: String,
    pub ingested_at: DateTime<Utc>,
    pub normalized_payload: Option<serde_json::Value>,
}

impl RecordSummary {
    pub fn is_normalized(&self) -> bool {
        self.normalized_payload.is_some()
    }
}

#[derive(Debug, Error)]
pub enum IngestionStatsClientError {
    #[error("ingestion service unreachable: {0}")]
    Unreachable(String),
    #[error("ingestion service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads per-connector ingestion volume/recency and per-connector record listings directly
/// from Ingestion Service — the data this platform's Agent status/drill-down views are built
/// on, since there is no separate "agent run" bookkeeping anywhere: a connector's own ingested
/// records are the ground truth for whether/when it ran.
#[async_trait]
pub trait IngestionStatsClient: Send + Sync {
    async fn connector_stats(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ConnectorStatSummary>, IngestionStatsClientError>;
    async fn records_by_connector(
        &self,
        tenant_id: Uuid,
        connector_id: &str,
    ) -> Result<Vec<RecordSummary>, IngestionStatsClientError>;
}

pub struct HttpIngestionStatsClient {
    client: reqwest::Client,
    ingestion_service_url: String,
}

impl HttpIngestionStatsClient {
    pub fn new(client: reqwest::Client, ingestion_service_url: String) -> Self {
        Self { client, ingestion_service_url }
    }
}

#[async_trait]
impl IngestionStatsClient for HttpIngestionStatsClient {
    async fn connector_stats(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ConnectorStatSummary>, IngestionStatsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/records/stats", self.ingestion_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IngestionStatsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))
    }

    async fn records_by_connector(
        &self,
        tenant_id: Uuid,
        connector_id: &str,
    ) -> Result<Vec<RecordSummary>, IngestionStatsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/records/by-connector", self.ingestion_service_url))
            .query(&[("connector_id", connector_id)])
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IngestionStatsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))
    }
}
