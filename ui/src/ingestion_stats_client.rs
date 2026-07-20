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
    #[serde(default)]
    pub connector_id: String,
    pub source_type: String,
    pub ingested_at: DateTime<Utc>,
    #[serde(default)]
    pub raw_payload: serde_json::Value,
    pub normalized_payload: Option<serde_json::Value>,
}

impl RecordSummary {
    pub fn is_normalized(&self) -> bool {
        self.normalized_payload.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct RecordSearchFilter {
    pub connector_id: Option<String>,
    pub source_type: Option<String>,
    pub query: Option<String>,
    pub subject: Option<String>,
    pub email_from: Option<String>,
    pub attachment_filename: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for RecordSearchFilter {
    fn default() -> Self {
        Self {
            connector_id: None,
            source_type: None,
            query: None,
            subject: None,
            email_from: None,
            attachment_filename: None,
            limit: DEFAULT_PAGE_SIZE,
            offset: 0,
        }
    }
}

pub const DEFAULT_PAGE_SIZE: i64 = 25;

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub records: Vec<RecordSummary>,
    pub has_more: bool,
}

#[derive(Debug, Error)]
pub enum IngestionStatsClientError {
    #[error("ingestion service unreachable: {0}")]
    Unreachable(String),
    #[error("ingestion service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads per-connector ingestion volume/recency and per-connector record listings directly
/// from Ingestion Service — the data this platform's Sensor status/drill-down views are built
/// on, since there is no separate "sensor run" bookkeeping anywhere: a connector's own ingested
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

    /// The Data Viewer's search — every filter is optional and AND-ed together.
    async fn search_records(
        &self,
        tenant_id: Uuid,
        filter: &RecordSearchFilter,
    ) -> Result<SearchResult, IngestionStatsClientError>;

    /// The Data Viewer's record detail view (full raw + normalized payload).
    async fn get_record(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<RecordSummary>, IngestionStatsClientError>;

    /// Republishes `record.ingested` for every one of this tenant's records with no
    /// `normalized_payload` yet — the recovery path for records ingested before a
    /// `NormalizationMapping` existed for their source type. Returns how many were
    /// republished (bounded to Ingestion Service's own per-call cap).
    async fn reprocess(&self, tenant_id: Uuid) -> Result<usize, IngestionStatsClientError>;
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

    async fn search_records(
        &self,
        tenant_id: Uuid,
        filter: &RecordSearchFilter,
    ) -> Result<SearchResult, IngestionStatsClientError> {
        let mut params: Vec<(&str, String)> =
            vec![("limit", filter.limit.to_string()), ("offset", filter.offset.to_string())];
        if let Some(connector_id) = &filter.connector_id {
            if !connector_id.is_empty() {
                params.push(("connector_id", connector_id.clone()));
            }
        }
        if let Some(source_type) = &filter.source_type {
            if !source_type.is_empty() {
                params.push(("source_type", source_type.clone()));
            }
        }
        if let Some(query) = &filter.query {
            if !query.is_empty() {
                params.push(("q", query.clone()));
            }
        }
        if let Some(subject) = &filter.subject {
            if !subject.is_empty() {
                params.push(("subject", subject.clone()));
            }
        }
        if let Some(email_from) = &filter.email_from {
            if !email_from.is_empty() {
                params.push(("email_from", email_from.clone()));
            }
        }
        if let Some(attachment_filename) = &filter.attachment_filename {
            if !attachment_filename.is_empty() {
                params.push(("attachment_filename", attachment_filename.clone()));
            }
        }

        let response = self
            .client
            .get(format!("{}/v1/records/search", self.ingestion_service_url))
            .query(&params)
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IngestionStatsClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct SearchRecordsResponse {
            records: Vec<RecordSummary>,
            has_more: bool,
        }
        let body: SearchRecordsResponse = response
            .json()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;
        Ok(SearchResult { records: body.records, has_more: body.has_more })
    }

    async fn get_record(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<RecordSummary>, IngestionStatsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/records/{id}", self.ingestion_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(IngestionStatsClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))
    }

    async fn reprocess(&self, tenant_id: Uuid) -> Result<usize, IngestionStatsClientError> {
        let response = self
            .client
            .post(format!("{}/v1/records/reprocess", self.ingestion_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IngestionStatsClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct ReprocessResponse {
            republished: usize,
        }
        let body: ReprocessResponse = response
            .json()
            .await
            .map_err(|e| IngestionStatsClientError::Unreachable(e.to_string()))?;
        Ok(body.republished)
    }
}
