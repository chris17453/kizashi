#[path = "saved_search_queries_client_test.rs"]
#[cfg(test)]
pub(crate) mod saved_search_queries_client_test;

use async_trait::async_trait;
use common::SavedSearchQuery;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SavedSearchQueriesClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads/writes `/data` page saved-search bookmarks via config-admin-service (ADR-0029) — same
/// direct-call trust boundary as `NormalizationMappingsClient`/`TriggersClient`, but no `Role`
/// gating: a saved search is a personal/team bookmark, not a write path that changes platform
/// behavior, so any authenticated tenant member can save/list/delete one.
#[async_trait]
pub trait SavedSearchQueriesClient: Send + Sync {
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueriesClientError>;

    async fn create(
        &self,
        tenant_id: Uuid,
        name: &str,
        filter: serde_json::Value,
    ) -> Result<SavedSearchQuery, SavedSearchQueriesClientError>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), SavedSearchQueriesClientError>;
}

pub struct HttpSavedSearchQueriesClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpSavedSearchQueriesClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl SavedSearchQueriesClient for HttpSavedSearchQueriesClient {
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueriesClientError> {
        let response = self
            .client
            .get(format!("{}/v1/saved-search-queries", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        name: &str,
        filter: serde_json::Value,
    ) -> Result<SavedSearchQuery, SavedSearchQueriesClientError> {
        let query = SavedSearchQuery::new(tenant_id, name, filter);
        let response = self
            .client
            .post(format!("{}/v1/saved-search-queries", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .json(&query)
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), SavedSearchQueriesClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/saved-search-queries/{id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }
}
