#[path = "sensor_repository_test.rs"]
#[cfg(test)]
pub(crate) mod sensor_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::Sensor;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SensorRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no sensor with id {0}")]
    NotFound(Uuid),
}

/// CRUD for Sensor, config-admin's own Postgres schema — same audit-logging convention as
/// TriggerDefinition/NormalizationMapping (CLAUDE.md §5): every create/update/delete writes one
/// audit_log row in the same transaction as the entity change.
#[async_trait]
pub trait SensorRepository: Send + Sync {
    async fn create(&self, sensor: Sensor) -> Result<Sensor, SensorRepositoryError>;
    async fn update(&self, sensor: Sensor) -> Result<Sensor, SensorRepositoryError>;
    async fn get(&self, tenant_id: Uuid, id: Uuid)
        -> Result<Option<Sensor>, SensorRepositoryError>;
    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Sensor>, SensorRepositoryError>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), SensorRepositoryError>;

    /// Looks up a sensor by its registered `name` — the join key Ingestion Gateway uses to
    /// enforce enabled/disabled status at ingest time (a sensor's `name` is what a deployed
    /// connector's own `CONNECTOR_ID` is set to, per `SensorsClient`'s documented convention).
    async fn find_by_name(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<Sensor>, SensorRepositoryError>;
}

pub struct PostgresSensorRepository {
    pool: sqlx::PgPool,
}

impl PostgresSensorRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type SensorRow = (Uuid, Uuid, String, String, serde_json::Value, bool);

fn row_to_sensor(row: SensorRow) -> Sensor {
    let (id, tenant_id, connector_type, name, config, enabled) = row;
    Sensor { id, tenant_id, connector_type, name, config, enabled }
}

#[async_trait]
impl SensorRepository for PostgresSensorRepository {
    async fn create(&self, sensor: Sensor) -> Result<Sensor, SensorRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            "INSERT INTO agents (id, tenant_id, connector_type, name, config, enabled) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(sensor.id)
        .bind(sensor.tenant_id)
        .bind(&sensor.connector_type)
        .bind(&sensor.name)
        .bind(&sensor.config)
        .bind(sensor.enabled)
        .execute(&mut *tx)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: sensor.tenant_id,
                entity_type: "agent".to_string(),
                entity_id: sensor.id,
                change_type: ChangeType::Created,
                actor: sensor.tenant_id.to_string(),
                before: None,
                after: serde_json::to_value(&sensor).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;
        Ok(sensor)
    }

    async fn update(&self, sensor: Sensor) -> Result<Sensor, SensorRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        let existing: Option<SensorRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(sensor.id)
        .bind(sensor.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(SensorRepositoryError::NotFound(sensor.id));
        };
        let before = row_to_sensor(existing);

        sqlx::query(
            "UPDATE agents SET connector_type = $1, name = $2, config = $3, enabled = $4 WHERE id = $5 AND tenant_id = $6",
        )
        .bind(&sensor.connector_type)
        .bind(&sensor.name)
        .bind(&sensor.config)
        .bind(sensor.enabled)
        .bind(sensor.id)
        .bind(sensor.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: sensor.tenant_id,
                entity_type: "agent".to_string(),
                entity_id: sensor.id,
                change_type: ChangeType::Updated,
                actor: sensor.tenant_id.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&sensor).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;
        Ok(sensor)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sensor>, SensorRepositoryError> {
        let row: Option<SensorRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_sensor))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Sensor>, SensorRepositoryError> {
        let rows: Vec<SensorRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE tenant_id = $1 ORDER BY name LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_sensor).collect())
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), SensorRepositoryError> {
        let mut tx =
            self.pool.begin().await.map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        let existing: Option<SensorRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        let Some(existing) = existing else {
            return Err(SensorRepositoryError::NotFound(id));
        };
        let before = row_to_sensor(existing);

        sqlx::query("DELETE FROM agents WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "agent".to_string(),
                entity_id: id,
                change_type: ChangeType::Deleted,
                actor: tenant_id.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::Value::Null,
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn find_by_name(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<Sensor>, SensorRepositoryError> {
        let row: Option<SensorRow> = sqlx::query_as(
            "SELECT id, tenant_id, connector_type, name, config, enabled FROM agents WHERE tenant_id = $1 AND name = $2",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SensorRepositoryError::Backend(e.to_string()))?;
        Ok(row.map(row_to_sensor))
    }
}
