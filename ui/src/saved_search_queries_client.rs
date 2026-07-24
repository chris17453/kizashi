#[path = "saved_search_queries_client_test.rs"]
#[cfg(test)]
pub(crate) mod saved_search_queries_client_test;

use async_trait::async_trait;
use common::{EventTypeDefinition, ReportRun, Role, SavedSearchQuery};
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

    /// Event-contract registry calls share the same Config/Admin connection and error boundary.
    /// Defaults keep lightweight test doubles source-compatible while production's HTTP client
    /// exposes the governed schema editor without adding another AppState dependency.
    async fn list_event_types(
        &self,
        _tenant_id: Uuid,
        _all_versions: bool,
    ) -> Result<Vec<EventTypeDefinition>, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Rejected(501))
    }

    async fn create_event_type(
        &self,
        _role: Role,
        _actor: &str,
        _definition: EventTypeDefinition,
    ) -> Result<EventTypeDefinition, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Rejected(501))
    }

    async fn create_event_type_version(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        _id: Uuid,
        _field_schema: serde_json::Value,
    ) -> Result<EventTypeDefinition, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Rejected(501))
    }

    async fn list_report_runs(
        &self,
        _tenant_id: Uuid,
        _schedule_id: Option<Uuid>,
    ) -> Result<Vec<ReportRun>, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Rejected(501))
    }
    async fn create_report_run(
        &self,
        _role: Role,
        _run: ReportRun,
    ) -> Result<ReportRun, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Rejected(501))
    }
    async fn update_report_run(
        &self,
        _role: Role,
        _run: ReportRun,
    ) -> Result<ReportRun, SavedSearchQueriesClientError> {
        Err(SavedSearchQueriesClientError::Rejected(501))
    }
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

    async fn list_event_types(
        &self,
        tenant_id: Uuid,
        all_versions: bool,
    ) -> Result<Vec<EventTypeDefinition>, SavedSearchQueriesClientError> {
        let response = self
            .client
            .get(format!("{}/v1/event-type-definitions", self.config_admin_service_url))
            .query(&[("all_versions", all_versions.to_string())])
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn create_event_type(
        &self,
        role: Role,
        actor: &str,
        definition: EventTypeDefinition,
    ) -> Result<EventTypeDefinition, SavedSearchQueriesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/event-type-definitions", self.config_admin_service_url))
            .header("x-tenant-id", definition.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&definition)
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn create_event_type_version(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        id: Uuid,
        field_schema: serde_json::Value,
    ) -> Result<EventTypeDefinition, SavedSearchQueriesClientError> {
        let response = self
            .client
            .post(format!(
                "{}/v1/event-type-definitions/{id}/versions",
                self.config_admin_service_url
            ))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({"field_schema": field_schema}))
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn list_report_runs(
        &self,
        tenant_id: Uuid,
        schedule_id: Option<Uuid>,
    ) -> Result<Vec<ReportRun>, SavedSearchQueriesClientError> {
        let mut request = self
            .client
            .get(format!("{}/v1/report-runs", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string());
        if let Some(schedule_id) = schedule_id {
            request = request.query(&[("schedule_id", schedule_id.to_string())]);
        }
        let response = request
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn create_report_run(
        &self,
        role: Role,
        run: ReportRun,
    ) -> Result<ReportRun, SavedSearchQueriesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/report-runs", self.config_admin_service_url))
            .header("x-tenant-id", run.tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&run)
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }

    async fn update_report_run(
        &self,
        role: Role,
        run: ReportRun,
    ) -> Result<ReportRun, SavedSearchQueriesClientError> {
        let response = self
            .client
            .put(format!("{}/v1/report-runs/{}", self.config_admin_service_url, run.id))
            .header("x-tenant-id", run.tenant_id.to_string())
            .header("x-role", role.to_string())
            .json(&run)
            .send()
            .await
            .map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(SavedSearchQueriesClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SavedSearchQueriesClientError::Unreachable(e.to_string()))
    }
}
