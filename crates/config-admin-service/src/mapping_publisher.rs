#[path = "mapping_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod mapping_publisher_test;

use async_trait::async_trait;
use common::{MappingChangeEvent, MAPPING_CHANGED_EXCHANGE};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MappingPublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize mapping for publish: {0}")]
    Serialization(String),
}

/// Publishes `mapping.changed` on every mapping create/update/delete, mirroring ADR-0018's
/// `trigger.changed` pattern (and ADR-0109/ADR-0110's `TriggerChangeEvent`/`MappingChangeEvent`
/// shape) — closes the gap ADR-0010 originally flagged for both trigger-engine and
/// normalization-service ("not yet the operational source of truth") but only ADR-0018 actually
/// closed, for triggers. Without this, an operator editing a Field Mapping through the Console
/// UI has zero effect on the running normalization pipeline until a manual DB write or restart.
/// A tagged `MappingChangeEvent` rather than always publishing a bare `NormalizationMapping`
/// because deletion has no mapping payload to carry.
#[async_trait]
pub trait MappingPublisher: Send + Sync {
    async fn publish_mapping_changed(
        &self,
        event: &MappingChangeEvent,
    ) -> Result<(), MappingPublishError>;
}

pub struct RabbitMqMappingPublisher {
    channel: Channel,
}

impl RabbitMqMappingPublisher {
    pub async fn new(channel: Channel) -> Result<Self, MappingPublishError> {
        channel
            .exchange_declare(
                MAPPING_CHANGED_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions { durable: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| MappingPublishError::Bus(e.to_string()))?;
        Ok(Self { channel })
    }
}

#[async_trait]
impl MappingPublisher for RabbitMqMappingPublisher {
    async fn publish_mapping_changed(
        &self,
        event: &MappingChangeEvent,
    ) -> Result<(), MappingPublishError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| MappingPublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                MAPPING_CHANGED_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| MappingPublishError::Bus(e.to_string()))?
            .await
            .map_err(|e| MappingPublishError::Bus(e.to_string()))?;
        Ok(())
    }
}
