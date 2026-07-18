#[path = "raw_record_client_test.rs"]
#[cfg(test)]
pub(crate) mod raw_record_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::RawRecord;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RawRecordClientError {
    #[error("ingestion service unreachable: {0}")]
    Unreachable(String),
    #[error("ingestion service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Retention Service's only path onto the raw store — never a direct Postgres connection into
/// Ingestion Service's schema (spec §2 principle 1, ADR-0011). `reimport` re-feeds an archived
/// record through Ingestion Service's normal ingest path, re-triggering `record.ingested` →
/// normalization → analysis exactly as ADR-0005 requires; see ADR-0011 point 4 for why this
/// calls Ingestion Service directly rather than through Ingestion Gateway.
#[async_trait]
pub trait RawRecordClient: Send + Sync {
    async fn list_older_than(
        &self,
        tenant_id: Uuid,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RawRecordClientError>;

    async fn delete(&self, tenant_id: Uuid, record_id: Uuid) -> Result<(), RawRecordClientError>;

    async fn reimport(&self, record: &RawRecord) -> Result<(), RawRecordClientError>;
}

pub struct HttpRawRecordClient {
    client: reqwest::Client,
    ingestion_service_url: String,
}

impl HttpRawRecordClient {
    pub fn new(client: reqwest::Client, ingestion_service_url: String) -> Self {
        Self { client, ingestion_service_url }
    }
}

#[async_trait]
impl RawRecordClient for HttpRawRecordClient {
    async fn list_older_than(
        &self,
        tenant_id: Uuid,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RawRecordClientError> {
        let response = self
            .client
            .get(format!("{}/v1/records", self.ingestion_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .query(&[("older_than", cutoff.to_rfc3339()), ("limit", limit.to_string())])
            .send()
            .await
            .map_err(|e| RawRecordClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(RawRecordClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RawRecordClientError::Unreachable(e.to_string()))
    }

    async fn delete(&self, tenant_id: Uuid, record_id: Uuid) -> Result<(), RawRecordClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/records/{}", self.ingestion_service_url, record_id))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| RawRecordClientError::Unreachable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(RawRecordClientError::Rejected(response.status().as_u16()))
        }
    }

    async fn reimport(&self, record: &RawRecord) -> Result<(), RawRecordClientError> {
        let response = self
            .client
            .post(format!("{}/v1/records", self.ingestion_service_url))
            .json(&serde_json::json!({
                "connector_id": record.connector_id,
                "source_type": record.source_type,
                "tenant_id": record.tenant_id,
                "raw_payload": record.raw_payload,
                "occurred_at": record.occurred_at,
            }))
            .send()
            .await
            .map_err(|e| RawRecordClientError::Unreachable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(RawRecordClientError::Rejected(response.status().as_u16()))
        }
    }
}
