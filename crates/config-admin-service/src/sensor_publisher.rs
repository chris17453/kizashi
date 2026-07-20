#[path = "sensor_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod sensor_publisher_test;

use async_trait::async_trait;
use common::{SensorChangeEvent, SENSOR_CHANGED_EXCHANGE};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SensorPublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize sensor change for publish: {0}")]
    Serialization(String),
}

/// Publishes `sensor.changed` on every Sensor create/update/delete (ADR-0020) so Sensor
/// Scheduler's own copy of the registry — the one it walks to decide what's due to poll —
/// stays in sync. Same pattern as `TriggerPublisher`/`AnalysisConfigPublisher`.
#[async_trait]
pub trait SensorPublisher: Send + Sync {
    async fn publish_sensor_changed(
        &self,
        event: &SensorChangeEvent,
    ) -> Result<(), SensorPublishError>;
}

pub struct RabbitMqSensorPublisher {
    channel: Channel,
}

impl RabbitMqSensorPublisher {
    pub async fn new(channel: Channel) -> Result<Self, SensorPublishError> {
        channel
            .exchange_declare(
                SENSOR_CHANGED_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions { durable: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| SensorPublishError::Bus(e.to_string()))?;
        Ok(Self { channel })
    }
}

#[async_trait]
impl SensorPublisher for RabbitMqSensorPublisher {
    async fn publish_sensor_changed(
        &self,
        event: &SensorChangeEvent,
    ) -> Result<(), SensorPublishError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| SensorPublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                SENSOR_CHANGED_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| SensorPublishError::Bus(e.to_string()))?
            .await
            .map_err(|e| SensorPublishError::Bus(e.to_string()))?;
        Ok(())
    }
}
