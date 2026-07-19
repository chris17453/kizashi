#[path = "retention_policies_client_test.rs"]
#[cfg(test)]
pub(crate) mod retention_policies_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    Raw,
    Normalized,
    Event,
}

impl std::fmt::Display for DataClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataClass::Raw => write!(f, "raw"),
            DataClass::Normalized => write!(f, "normalized"),
            DataClass::Event => write!(f, "event"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RetentionPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub data_class: DataClass,
    pub ttl_days: i32,
    pub enabled: bool,
}

#[derive(Debug, Error)]
pub enum RetentionPoliciesClientError {
    #[error("retention service unreachable: {0}")]
    Unreachable(String),
    #[error("retention service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads/writes RetentionPolicy via retention-service directly — same direct-call trust
/// boundary as `TriggersClient`/`NormalizationMappingsClient` (`x-tenant-id`/`x-role` headers,
/// no gateway in front of retention-service, ADR-0011). This entity previously had zero
/// Console UI presence despite having had a full CRUD + RBAC-enforced API since ADR-0011/its
/// RBAC follow-up — spec §7's "data lifecycle UI" line item.
#[async_trait]
pub trait RetentionPoliciesClient: Send + Sync {
    async fn list_policies(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPoliciesClientError>;

    async fn create_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError>;

    /// Persists `policy` as-is via `PUT /v1/retention-policies/:id` — used for both the
    /// enable/disable toggle and the TTL edit form, matching `AgentsClient::update_agent`'s
    /// convention.
    async fn update_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError>;

    /// Deletes a policy via `DELETE /v1/retention-policies/:id`, matching
    /// `AgentsClient::delete_agent`'s convention.
    async fn delete_policy(
        &self,
        role: Role,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), RetentionPoliciesClientError>;
}

pub struct HttpRetentionPoliciesClient {
    client: reqwest::Client,
    retention_service_url: String,
}

impl HttpRetentionPoliciesClient {
    pub fn new(client: reqwest::Client, retention_service_url: String) -> Self {
        Self { client, retention_service_url }
    }
}

#[async_trait]
impl RetentionPoliciesClient for HttpRetentionPoliciesClient {
    async fn list_policies(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RetentionPolicy>, RetentionPoliciesClientError> {
        let response = self
            .client
            .get(format!("{}/v1/retention-policies", self.retention_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }

    async fn create_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/retention-policies", self.retention_service_url))
            .header("x-tenant-id", policy.tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&policy)
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }

    async fn update_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        let response = self
            .client
            .put(format!("{}/v1/retention-policies/{}", self.retention_service_url, policy.id))
            .header("x-tenant-id", policy.tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&policy)
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }

    async fn delete_policy(
        &self,
        role: Role,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), RetentionPoliciesClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/retention-policies/{id}", self.retention_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }
}
