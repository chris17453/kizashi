#[path = "connector_test.rs"]
#[cfg(test)]
mod connector_test;

use async_trait::async_trait;
use common::connector::{Connector, ConnectorError};
use common::raw_record::{RawRecord, SourceType};

/// A configurable HTTP polling connector for sources with no dedicated integration yet
/// (spec §1 "config over code" — connector configuration, not a new connector crate, is the
/// default path for adding a new source). Polls a JSON array endpoint and maps each element
/// to a `RawRecord`, verbatim as `raw_payload` — no source-specific shape assumed.
pub struct GenericConnector {
    connector_id: String,
    client: reqwest::Client,
    source_url: String,
    bearer_token: Option<String>,
}

impl GenericConnector {
    pub fn new(
        connector_id: impl Into<String>,
        client: reqwest::Client,
        source_url: impl Into<String>,
        bearer_token: Option<String>,
    ) -> Self {
        Self {
            connector_id: connector_id.into(),
            client,
            source_url: source_url.into(),
            bearer_token,
        }
    }
}

#[async_trait]
impl Connector for GenericConnector {
    fn connector_id(&self) -> &str {
        &self.connector_id
    }

    fn source_type(&self) -> SourceType {
        SourceType::Generic
    }

    async fn poll(&self, tenant_id: uuid::Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        let mut request = self.client.get(&self.source_url);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }

        let response =
            request.send().await.map_err(|e| ConnectorError::SourceUnavailable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ConnectorError::AuthFailed(format!(
                "source rejected credentials: HTTP {}",
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

        let items: Vec<serde_json::Value> =
            response.json().await.map_err(|e| ConnectorError::MalformedRecord(e.to_string()))?;

        Ok(items
            .into_iter()
            .map(|item| {
                RawRecord::new(self.connector_id.clone(), SourceType::Generic, tenant_id, item)
            })
            .collect())
    }
}
