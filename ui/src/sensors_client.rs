#[path = "sensors_client_test.rs"]
#[cfg(test)]
pub(crate) mod sensors_client_test;

use async_trait::async_trait;
use common::{Role, Sensor};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SensorsClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorsPage {
    pub sensors: Vec<Sensor>,
    pub has_more: bool,
}

/// Registers/lists/deletes Sensors (a tenant's registered connector instances) via Config/Admin
/// Service, same direct-call trust boundary as `TriggersClient` (no gateway sits in front of
/// Config/Admin Service, ADR-0010). The operational convention this establishes: the sensor's
/// registered `name` is what the corresponding connector's own `CONNECTOR_ID` env var must be
/// set to, so ingested records (`connector_id`) can be matched back to a registered sensor for
/// status/drill-down — see `IngestionStatsClient`.
#[async_trait]
pub trait SensorsClient: Send + Sync {
    async fn list_sensors(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<SensorsPage, SensorsClientError>;

    /// Fetches a single sensor by id — used wherever a specific sensor is needed (detail view,
    /// toggle), rather than paging through `list_sensors` and hoping the id is on the current
    /// page.
    async fn get_sensor(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sensor>, SensorsClientError>;

    /// Registers a new sensor — operator-only (RBAC v1, ADR-0016 follow-up). `actor` is the
    /// signed-in session's username, sent as `X-Username` so config-admin-service can record the
    /// real actor on the audit-log entry instead of just the tenant.
    async fn register_sensor(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        connector_type: &str,
        name: &str,
        config: serde_json::Value,
    ) -> Result<Sensor, SensorsClientError>;

    /// Deletes a sensor — operator-only (RBAC v1, ADR-0016 follow-up).
    async fn delete_sensor(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), SensorsClientError>;

    /// Persists `sensor` as-is via `PUT /v1/sensors/:id` — used for the enable/disable toggle
    /// (flip `sensor.enabled`, then call this with the rest of the fields unchanged).
    /// Operator-only (RBAC v1, ADR-0016 follow-up).
    async fn update_sensor(
        &self,
        role: Role,
        actor: &str,
        sensor: &Sensor,
    ) -> Result<Sensor, SensorsClientError>;
}

pub struct HttpSensorsClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpSensorsClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }
}

#[async_trait]
impl SensorsClient for HttpSensorsClient {
    async fn list_sensors(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<SensorsPage, SensorsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/sensors", self.config_admin_service_url))
            .query(&[("limit", limit.to_string()), ("offset", offset.to_string())])
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| SensorsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SensorsClientError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct ListSensorsResponse {
            sensors: Vec<Sensor>,
            has_more: bool,
        }
        let body: ListSensorsResponse =
            response.json().await.map_err(|e| SensorsClientError::Unreachable(e.to_string()))?;
        Ok(SensorsPage { sensors: body.sensors, has_more: body.has_more })
    }

    async fn get_sensor(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sensor>, SensorsClientError> {
        let response = self
            .client
            .get(format!("{}/v1/sensors/{id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| SensorsClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(SensorsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map(Some).map_err(|e| SensorsClientError::Unreachable(e.to_string()))
    }

    async fn register_sensor(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        connector_type: &str,
        name: &str,
        config: serde_json::Value,
    ) -> Result<Sensor, SensorsClientError> {
        let sensor = Sensor::new(tenant_id, connector_type, name, config);
        let response = self
            .client
            .post(format!("{}/v1/sensors", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&sensor)
            .send()
            .await
            .map_err(|e| SensorsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SensorsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SensorsClientError::Unreachable(e.to_string()))
    }

    async fn delete_sensor(
        &self,
        role: Role,
        actor: &str,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), SensorsClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/sensors/{id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| SensorsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SensorsClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }

    async fn update_sensor(
        &self,
        role: Role,
        actor: &str,
        sensor: &Sensor,
    ) -> Result<Sensor, SensorsClientError> {
        let response = self
            .client
            .put(format!("{}/v1/sensors/{}", self.config_admin_service_url, sensor.id))
            .header("x-tenant-id", sensor.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(sensor)
            .send()
            .await
            .map_err(|e| SensorsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SensorsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| SensorsClientError::Unreachable(e.to_string()))
    }
}
