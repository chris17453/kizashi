#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_trait::async_trait;
use common::connector::{Connector, ConnectorError};
use common::raw_record::{RawRecord, SourceType};
use serde::Deserialize;

/// Polls Zendesk's Incremental Ticket Export API
/// (`GET /api/v2/incremental/tickets.json?start_time=`), authenticated via the
/// `{email}/token:{api_token}` HTTP Basic scheme Zendesk's API token auth uses. `start_time`
/// (a Unix timestamp) is operator-configured rather than tracked across runs, matching the sql
/// connector's stateless-cursor design (ADR-0013) — each CronJob invocation is a fresh process.
pub struct ZendeskConnector {
    connector_id: String,
    client: reqwest::Client,
    base_url: String,
    email: String,
    api_token: String,
    start_time: i64,
}

impl ZendeskConnector {
    pub fn new(
        connector_id: impl Into<String>,
        client: reqwest::Client,
        base_url: impl Into<String>,
        email: impl Into<String>,
        api_token: impl Into<String>,
        start_time: i64,
    ) -> Self {
        Self {
            connector_id: connector_id.into(),
            client,
            base_url: base_url.into(),
            email: email.into(),
            api_token: api_token.into(),
            start_time,
        }
    }
}

#[derive(Debug, Deserialize)]
struct IncrementalTicketsResponse {
    tickets: Vec<serde_json::Value>,
}

#[async_trait]
impl Connector for ZendeskConnector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::Ticket
    }

    async fn poll(&self, tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        let response = self
            .client
            .get(format!("{}/api/v2/incremental/tickets.json", self.base_url))
            .query(&[("start_time", self.start_time.to_string())])
            .basic_auth(format!("{}/token", self.email), Some(&self.api_token))
            .send()
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ConnectorError::AuthFailed(format!(
                "Zendesk rejected credentials: HTTP {}",
                response.status()
            )));
        }
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after_secs = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);
            return Err(ConnectorError::RateLimited { retry_after_secs });
        }
        if !response.status().is_success() {
            return Err(ConnectorError::SourceUnavailable(format!(
                "unexpected status {}",
                response.status()
            )));
        }

        let body: IncrementalTicketsResponse =
            response.json().await.map_err(|e| ConnectorError::MalformedRecord(e.to_string()))?;

        Ok(body
            .tickets
            .into_iter()
            .map(|ticket| {
                RawRecord::new(self.connector_id.clone(), SourceType::Ticket, tenant_id, ticket)
            })
            .collect())
    }
}
