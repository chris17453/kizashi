#[path = "incidents_client_test.rs"]
#[cfg(test)]
pub(crate) mod incidents_client_test;

use async_trait::async_trait;
use common::{Incident, IncidentStatus, Role};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum IncidentsClientError {
    #[error("incident service unreachable: {0}")]
    Unreachable(String),
    #[error("incident service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct IncidentDetail {
    #[serde(flatten)]
    pub incident: Incident,
    pub event_ids: Vec<Uuid>,
}

/// Reads/writes Incidents via incident-service (ADR-0111) — same direct-call trust boundary as
/// `TriggersClient` (`x-tenant-id`/`x-role`/`x-username` headers, no gateway sits in front of
/// incident-service).
#[async_trait]
pub trait IncidentsClient: Send + Sync {
    async fn list_incidents(
        &self,
        tenant_id: Uuid,
        status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<IncidentDetail>, IncidentsClientError>;

    async fn get_incident(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<IncidentDetail>, IncidentsClientError>;

    /// Creates an incident, optionally linking `initial_event_ids` in the same call — the
    /// "create incident from selected Events" bulk action on the Events page.
    async fn create_incident(
        &self,
        role: Role,
        actor: &str,
        incident: Incident,
        initial_event_ids: Vec<Uuid>,
    ) -> Result<IncidentDetail, IncidentsClientError>;

    async fn update_incident(
        &self,
        role: Role,
        actor: &str,
        incident: Incident,
    ) -> Result<IncidentDetail, IncidentsClientError>;

    async fn link_event(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), IncidentsClientError>;

    async fn unlink_event(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), IncidentsClientError>;
}

pub struct HttpIncidentsClient {
    client: reqwest::Client,
    incident_service_url: String,
}

impl HttpIncidentsClient {
    pub fn new(client: reqwest::Client, incident_service_url: String) -> Self {
        Self { client, incident_service_url }
    }
}

#[async_trait]
impl IncidentsClient for HttpIncidentsClient {
    async fn list_incidents(
        &self,
        tenant_id: Uuid,
        status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<IncidentDetail>, IncidentsClientError> {
        let mut request = self
            .client
            .get(format!("{}/v1/incidents", self.incident_service_url))
            .header("x-tenant-id", tenant_id.to_string());
        if let Some(status) = status_filter {
            request = request.query(&[("status", status.to_string())]);
        }
        let response =
            request.send().await.map_err(|e| IncidentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IncidentsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| IncidentsClientError::Unreachable(e.to_string()))
    }

    async fn get_incident(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<IncidentDetail>, IncidentsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/incidents/{id}", self.incident_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| IncidentsClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(IncidentsClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| IncidentsClientError::Unreachable(e.to_string()))
    }

    async fn create_incident(
        &self,
        role: Role,
        actor: &str,
        incident: Incident,
        initial_event_ids: Vec<Uuid>,
    ) -> Result<IncidentDetail, IncidentsClientError> {
        let body = serde_json::json!({
            "id": incident.id,
            "tenant_id": incident.tenant_id,
            "title": incident.title,
            "summary": incident.summary,
            "severity": incident.severity.to_string(),
            "status": incident.status.to_string(),
            "created_at": incident.created_at,
            "updated_at": incident.updated_at,
            "resolved_at": incident.resolved_at,
            "initial_event_ids": initial_event_ids,
        });
        let response = self
            .client
            .post(format!("{}/v1/incidents", self.incident_service_url))
            .header("x-tenant-id", incident.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&body)
            .send()
            .await
            .map_err(|e| IncidentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IncidentsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| IncidentsClientError::Unreachable(e.to_string()))
    }

    async fn update_incident(
        &self,
        role: Role,
        actor: &str,
        incident: Incident,
    ) -> Result<IncidentDetail, IncidentsClientError> {
        let body = serde_json::json!({
            "id": incident.id,
            "tenant_id": incident.tenant_id,
            "title": incident.title,
            "summary": incident.summary,
            "severity": incident.severity.to_string(),
            "status": incident.status.to_string(),
            "created_at": incident.created_at,
            "updated_at": incident.updated_at,
            "resolved_at": incident.resolved_at,
        });
        let response = self
            .client
            .put(format!("{}/v1/incidents/{}", self.incident_service_url, incident.id))
            .header("x-tenant-id", incident.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&body)
            .send()
            .await
            .map_err(|e| IncidentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IncidentsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| IncidentsClientError::Unreachable(e.to_string()))
    }

    async fn link_event(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), IncidentsClientError> {
        let response = self
            .client
            .post(format!("{}/v1/incidents/{incident_id}/events", self.incident_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({ "event_id": event_id }))
            .send()
            .await
            .map_err(|e| IncidentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IncidentsClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }

    async fn unlink_event(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), IncidentsClientError> {
        let response = self
            .client
            .delete(format!(
                "{}/v1/incidents/{incident_id}/events/{event_id}",
                self.incident_service_url
            ))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| IncidentsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IncidentsClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }
}
