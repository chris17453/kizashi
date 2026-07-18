#[path = "event_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod event_publisher_test;

use async_trait::async_trait;
use common::{Event, EVENT_CREATED_EXCHANGE};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize event for publish: {0}")]
    Serialization(String),
}

/// Publishes `event.created` once a firing TriggerDefinition's Event has been durably written
/// to ClickHouse (spec §3).
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish_event_created(&self, event: &Event) -> Result<(), PublishError>;
}

pub struct RabbitMqEventPublisher {
    channel: Channel,
}

impl RabbitMqEventPublisher {
    pub async fn new(channel: Channel) -> Result<Self, PublishError> {
        channel
            .exchange_declare(
                EVENT_CREATED_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions { durable: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| PublishError::Bus(e.to_string()))?;
        Ok(Self { channel })
    }
}

#[async_trait]
impl EventPublisher for RabbitMqEventPublisher {
    async fn publish_event_created(&self, event: &Event) -> Result<(), PublishError> {
        let payload =
            serde_json::to_vec(event).map_err(|e| PublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                EVENT_CREATED_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| PublishError::Bus(e.to_string()))?
            .await
            .map_err(|e| PublishError::Bus(e.to_string()))?;
        Ok(())
    }
}
