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
    async fn list_events(&self, bearer_token: &str)
        -> Result<Vec<EventSummary>, EventsClientError>;
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
    ) -> Result<Vec<EventSummary>, EventsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/events", self.query_gateway_url))
            .bearer_auth(bearer_token)
            .send()
            .await
            .map_err(|e| EventsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(EventsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| EventsClientError::Unreachable(e.to_string()))
    }
}
