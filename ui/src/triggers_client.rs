#[path = "triggers_client_test.rs"]
#[cfg(test)]
pub(crate) mod triggers_client_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct TriggerSummary {
    pub id: Uuid,
    pub name: String,
    pub event_type_match: String,
    pub enabled: bool,
}

#[derive(Debug, Error)]
pub enum TriggersClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggersPage {
    pub triggers: Vec<TriggerSummary>,
    pub has_more: bool,
}

/// Reads trigger definitions from Config/Admin Service (spec §6, service #11), trusting
/// `X-Tenant-Id` the same way every other internal-to-internal caller in this codebase does —
/// no gateway sits in front of Config/Admin Service (ADR-0010), so Console UI's backend calls
/// it directly with the tenant_id from the signed-in session.
#[async_trait]
pub trait TriggersClient: Send + Sync {
    async fn list_triggers(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<TriggersPage, TriggersClientError>;
}

pub struct HttpTriggersClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpTriggersClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl TriggersClient for HttpTriggersClient {
    async fn list_triggers(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<TriggersPage, TriggersClientError> {
        let response = self
            .client
            .get(format!("{}/v1/trigger-definitions", self.config_admin_service_url))
            .query(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| TriggersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TriggersClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct ListTriggersResponse {
            triggers: Vec<TriggerSummary>,
            has_more: bool,
        }
        let body: ListTriggersResponse =
            response.json().await.map_err(|e| TriggersClientError::Unreachable(e.to_string()))?;
        Ok(TriggersPage { triggers: body.triggers, has_more: body.has_more })
    }
}
