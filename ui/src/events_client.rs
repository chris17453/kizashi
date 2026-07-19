#[path = "events_client_test.rs"]
#[cfg(test)]
pub(crate) mod events_client_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct EventSummary {
    pub id: Uuid,
    pub event_type: String,
    pub group_key: String,
    pub status: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventsPage {
    pub events: Vec<EventSummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct DailyCount {
    pub date: String,
    pub count: u64,
}

#[derive(Debug, Error)]
pub enum EventsClientError {
    #[error("query gateway unreachable: {0}")]
    Unreachable(String),
    #[error("query gateway rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads events through Query Gateway (spec §6, service #8) using the signed-in session's
/// bearer token — the same trust boundary any other Query Gateway client uses.
#[async_trait]
pub trait EventsClient: Send + Sync {
    async fn list_events(
        &self,
        bearer_token: &str,
        limit: u32,
        offset: u32,
    ) -> Result<EventsPage, EventsClientError>;

    /// Lists the Events whose `record_ids` contain `record_id` — the record→event lineage
    /// lookup (ADR-0017) a record-journey view uses to find what a record contributed to.
    async fn list_events_for_record(
        &self,
        bearer_token: &str,
        record_id: Uuid,
    ) -> Result<Vec<EventSummary>, EventsClientError>;

    /// Daily event counts over `[since, until]` — the Events page's over-time chart.
    async fn daily_counts(
        &self,
        bearer_token: &str,
        since: chrono::DateTime<chrono::Utc>,
        until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<DailyCount>, EventsClientError>;
}

pub struct HttpEventsClient {
    client: reqwest::Client,
    query_gateway_url: String,
}

impl HttpEventsClient {
    pub fn new(client: reqwest::Client, query_gateway_url: String) -> Self {
        Self { client, query_gateway_url }
    }
}

#[async_trait]
impl EventsClient for HttpEventsClient {
    async fn list_events(
        &self,
        bearer_token: &str,
        limit: u32,
        offset: u32,
    ) -> Result<EventsPage, EventsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/events", self.query_gateway_url))
            .query(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .bearer_auth(bearer_token)
            .send()
            .await
            .map_err(|e| EventsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(EventsClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct ListEventsResponse {
            events: Vec<EventSummary>,
            has_more: bool,
        }
        let body: ListEventsResponse =
            response.json().await.map_err(|e| EventsClientError::Unreachable(e.to_string()))?;
        Ok(EventsPage { events: body.events, has_more: body.has_more })
    }

    async fn list_events_for_record(
        &self,
        bearer_token: &str,
        record_id: Uuid,
    ) -> Result<Vec<EventSummary>, EventsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/events", self.query_gateway_url))
            .query(&[("record_id", record_id.to_string()), ("limit", "100".to_string())])
            .bearer_auth(bearer_token)
            .send()
            .await
            .map_err(|e| EventsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(EventsClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct ListEventsResponse {
            events: Vec<EventSummary>,
        }
        let body: ListEventsResponse =
            response.json().await.map_err(|e| EventsClientError::Unreachable(e.to_string()))?;
        Ok(body.events)
    }

    async fn daily_counts(
        &self,
        bearer_token: &str,
        since: chrono::DateTime<chrono::Utc>,
        until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<DailyCount>, EventsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/events/daily-counts", self.query_gateway_url))
            .query(&[("since", since.to_rfc3339()), ("until", until.to_rfc3339())])
            .bearer_auth(bearer_token)
            .send()
            .await
            .map_err(|e| EventsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(EventsClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct DailyCountsResponse {
            counts: Vec<DailyCount>,
        }
        let body: DailyCountsResponse =
            response.json().await.map_err(|e| EventsClientError::Unreachable(e.to_string()))?;
        Ok(body.counts)
    }
}
