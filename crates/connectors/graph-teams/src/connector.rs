#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_trait::async_trait;
use common::connector::{Connector, ConnectorError};
use common::raw_record::{RawRecord, SourceType};
use connector_runtime::fetch_access_token;
use serde::Deserialize;

const GRAPH_SCOPE: &str = "https://graph.microsoft.com/.default";

/// Polls a Teams channel's messages via Microsoft Graph
/// (`GET /teams/{team_id}/channels/{channel_id}/messages`), same Entra app-only auth as
/// `graph-mail` (ADR-0003) — one app registration typically covers both Graph connectors for a
/// tenant, but each connector fetches its own token since they're independent CronJob
/// invocations (ADR-0013).
pub struct GraphTeamsConnector {
    connector_id: String,
    client: reqwest::Client,
    graph_base_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    team_id: String,
    channel_id: String,
}

impl GraphTeamsConnector {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connector_id: impl Into<String>,
        client: reqwest::Client,
        graph_base_url: impl Into<String>,
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        team_id: impl Into<String>,
        channel_id: impl Into<String>,
    ) -> Self {
        Self {
            connector_id: connector_id.into(),
            client,
            graph_base_url: graph_base_url.into(),
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            team_id: team_id.into(),
            channel_id: channel_id.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GraphListResponse {
    value: Vec<serde_json::Value>,
}

#[async_trait]
impl Connector for GraphTeamsConnector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::Message
    }

    async fn poll(&self, tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        let access_token =
            fetch_access_token(&self.token_url, &self.client_id, &self.client_secret, GRAPH_SCOPE)
                .await
                .map_err(|e| ConnectorError::AuthFailed(e.to_string()))?;

        let response = self
            .client
            .get(format!(
                "{}/teams/{}/channels/{}/messages",
                self.graph_base_url, self.team_id, self.channel_id
            ))
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ConnectorError::AuthFailed(format!(
                "Graph rejected the request: HTTP {}",
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

        let body: GraphListResponse =
            response.json().await.map_err(|e| ConnectorError::MalformedRecord(e.to_string()))?;

        Ok(body
            .value
            .into_iter()
            .map(|message| {
                RawRecord::new(self.connector_id.clone(), SourceType::Message, tenant_id, message)
            })
            .collect())
    }
}
