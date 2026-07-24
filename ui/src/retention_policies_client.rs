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

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ReimportSummary {
    pub records_reimported: usize,
    pub records_failed: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ComplianceHold {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub data_class: DataClass,
    pub reason: String,
    pub active: bool,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
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
        actor: &str,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError>;

    /// Persists `policy` as-is via `PUT /v1/retention-policies/:id` — used for both the
    /// enable/disable toggle and the TTL edit form, matching `AgentsClient::update_agent`'s
    /// convention.
    async fn update_policy(
        &self,
        role: Role,
        policy: RetentionPolicy,
        actor: &str,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError>;

    /// Deletes a policy via `DELETE /v1/retention-policies/:id`, matching
    /// `AgentsClient::delete_agent`'s convention.
    async fn delete_policy(
        &self,
        role: Role,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), RetentionPoliciesClientError>;

    async fn reimport_archive(
        &self,
        _tenant_id: Uuid,
        _archive_key: &str,
    ) -> Result<ReimportSummary, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Rejected(501))
    }

    async fn list_holds(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ComplianceHold>, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Rejected(501))
    }
    async fn create_hold(
        &self,
        _role: Role,
        _tenant_id: Uuid,
        _data_class: DataClass,
        _reason: &str,
        _actor: &str,
    ) -> Result<ComplianceHold, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Rejected(501))
    }
    async fn release_hold(
        &self,
        _role: Role,
        _tenant_id: Uuid,
        _id: Uuid,
        _actor: &str,
    ) -> Result<ComplianceHold, RetentionPoliciesClientError> {
        Err(RetentionPoliciesClientError::Rejected(501))
    }
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
        actor: &str,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/retention-policies", self.retention_service_url))
            .header("x-tenant-id", policy.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
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
        actor: &str,
    ) -> Result<RetentionPolicy, RetentionPoliciesClientError> {
        let response = self
            .client
            .put(format!("{}/v1/retention-policies/{}", self.retention_service_url, policy.id))
            .header("x-tenant-id", policy.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
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
        actor: &str,
    ) -> Result<(), RetentionPoliciesClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/retention-policies/{id}", self.retention_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }

    async fn reimport_archive(
        &self,
        tenant_id: Uuid,
        archive_key: &str,
    ) -> Result<ReimportSummary, RetentionPoliciesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/reimport", self.retention_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .json(&serde_json::json!({"archive_key": archive_key}))
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }

    async fn list_holds(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ComplianceHold>, RetentionPoliciesClientError> {
        let response = self
            .client
            .get(format!("{}/v1/compliance-holds", self.retention_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }

    async fn create_hold(
        &self,
        role: Role,
        tenant_id: Uuid,
        data_class: DataClass,
        reason: &str,
        actor: &str,
    ) -> Result<ComplianceHold, RetentionPoliciesClientError> {
        let response = self.client.post(format!("{}/v1/compliance-holds", self.retention_service_url)).header("x-tenant-id", tenant_id.to_string()).header("x-role", role.to_string()).header("x-username", actor).json(&serde_json::json!({"tenant_id": tenant_id, "data_class": data_class, "reason": reason})).send().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }

    async fn release_hold(
        &self,
        role: Role,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<ComplianceHold, RetentionPoliciesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/compliance-holds/{id}/release", self.retention_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(RetentionPoliciesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| RetentionPoliciesClientError::Unreachable(e.to_string()))
    }
}
