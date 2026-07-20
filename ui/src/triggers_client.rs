#[path = "triggers_client_test.rs"]
#[cfg(test)]
pub(crate) mod triggers_client_test;

use async_trait::async_trait;
use common::{Role, TriggerDefinition};
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

    /// Creates a trigger — operator-only (RBAC v1, ADR-0016); `trigger.tenant_id` must already
    /// be set to the calling session's tenant (config-admin-service rejects a mismatch). `actor`
    /// is the signed-in session's username, sent as `X-Username` so config-admin-service can
    /// record the real actor on the audit-log entry instead of just the tenant.
    async fn create_trigger(
        &self,
        role: Role,
        actor: &str,
        trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggersClientError>;

    /// Dry-runs a trigger against real, already-recorded signal history for `group_key`
    /// (ADR-0030) — calls `trigger-engine` directly, not config-admin-service, since
    /// trigger-engine is what owns `SignalRepository`/evaluation. No role gate: read-only.
    async fn test_trigger(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        group_key: &str,
    ) -> Result<TriggerTestResult, TriggersClientError>;
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct TriggerTestResult {
    pub would_fire: bool,
    pub contributing_record_count: usize,
}

pub struct HttpTriggersClient {
    client: reqwest::Client,
    config_admin_service_url: String,
    trigger_engine_url: String,
}

impl HttpTriggersClient {
    pub fn new(
        client: reqwest::Client,
        config_admin_service_url: String,
        trigger_engine_url: String,
    ) -> Self {
        Self { client, config_admin_service_url, trigger_engine_url }
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

    async fn create_trigger(
        &self,
        role: Role,
        actor: &str,
        trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggersClientError> {
        let response = self
            .client
            .post(format!("{}/v1/trigger-definitions", self.config_admin_service_url))
            .header("x-tenant-id", trigger.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&trigger)
            .send()
            .await
            .map_err(|e| TriggersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TriggersClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| TriggersClientError::Unreachable(e.to_string()))
    }

    async fn test_trigger(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        group_key: &str,
    ) -> Result<TriggerTestResult, TriggersClientError> {
        let response = self
            .client
            .post(format!("{}/v1/triggers/{id}/test", self.trigger_engine_url))
            .header("x-tenant-id", tenant_id.to_string())
            .json(&serde_json::json!({"group_key": group_key}))
            .send()
            .await
            .map_err(|e| TriggersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(TriggersClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| TriggersClientError::Unreachable(e.to_string()))
    }
}
