#[path = "normalization_mappings_client_test.rs"]
#[cfg(test)]
pub(crate) mod normalization_mappings_client_test;

use async_trait::async_trait;
use common::{NormalizationMapping, Role};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum NormalizationMappingsClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads/writes NormalizationMapping via config-admin-service — same direct-call trust
/// boundary as `TriggersClient` (`x-tenant-id`/`x-role` headers, no gateway sits in front of
/// config-admin-service).
#[async_trait]
pub trait NormalizationMappingsClient: Send + Sync {
    async fn list_mappings(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingsClientError>;

    /// `actor` is the signed-in session's username, sent as `X-Username` so config-admin-service
    /// can record the real actor on the audit-log entry instead of just the tenant.
    async fn create_mapping(
        &self,
        role: Role,
        actor: &str,
        mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingsClientError>;
}

pub struct HttpNormalizationMappingsClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpNormalizationMappingsClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl NormalizationMappingsClient for HttpNormalizationMappingsClient {
    async fn list_mappings(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/normalization-mappings", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| NormalizationMappingsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(NormalizationMappingsClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|e| NormalizationMappingsClientError::Unreachable(e.to_string()))
    }

    async fn create_mapping(
        &self,
        role: Role,
        actor: &str,
        mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingsClientError> {
        let response = self
            .client
            .post(format!("{}/v1/normalization-mappings", self.config_admin_service_url))
            .header("x-tenant-id", mapping.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&mapping)
            .send()
            .await
            .map_err(|e| NormalizationMappingsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(NormalizationMappingsClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|e| NormalizationMappingsClientError::Unreachable(e.to_string()))
    }
}
