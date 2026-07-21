#[path = "trigger_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod trigger_publisher_test;

use async_trait::async_trait;
use common::{TriggerChangeEvent, TRIGGER_CHANGED_EXCHANGE};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TriggerPublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize trigger for publish: {0}")]
    Serialization(String),
}

/// Publishes `trigger.changed` on every trigger create/update/delete (ADR-0018, ADR-0109) so
/// trigger-engine's own copy of trigger definitions — the one it actually evaluates against
/// every `record.analyzed` message — stays in sync with what operators author through this
/// service's API/Console UI. A tagged `TriggerChangeEvent` rather than always publishing a
/// bare `TriggerDefinition` because deletion has no definition payload to carry (same shape as
/// `SensorPublisher`/`SensorChangeEvent`).
#[async_trait]
pub trait TriggerPublisher: Send + Sync {
    async fn publish_trigger_changed(
        &self,
        event: &TriggerChangeEvent,
    ) -> Result<(), TriggerPublishError>;
}

pub struct RabbitMqTriggerPublisher {
    channel: Channel,
}

impl RabbitMqTriggerPublisher {
    pub async fn new(channel: Channel) -> Result<Self, TriggerPublishError> {
        channel
            .exchange_declare(
                TRIGGER_CHANGED_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions { durable: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| TriggerPublishError::Bus(e.to_string()))?;
        Ok(Self { channel })
    }
}

#[async_trait]
impl TriggerPublisher for RabbitMqTriggerPublisher {
    async fn publish_trigger_changed(
        &self,
        event: &TriggerChangeEvent,
    ) -> Result<(), TriggerPublishError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| TriggerPublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                TRIGGER_CHANGED_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| TriggerPublishError::Bus(e.to_string()))?
            .await
            .map_err(|e| TriggerPublishError::Bus(e.to_string()))?;
        Ok(())
    }
}
