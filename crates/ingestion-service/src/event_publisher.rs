#[path = "event_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod event_publisher_test;

use async_trait::async_trait;
use common::RawRecord;
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

pub const RECORD_INGESTED_EXCHANGE: &str = "record.ingested";

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize record for publish: {0}")]
    Serialization(String),
}

/// Publishes `record.ingested` once a RawRecord is durably persisted (spec §3). Abstracted
/// behind a trait so ingest_handler's unit tests don't need a live RabbitMQ (CLAUDE.md §2).
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish_record_ingested(&self, record: &RawRecord) -> Result<(), PublishError>;
}

pub struct RabbitMqEventPublisher {
    channel: Channel,
}

impl RabbitMqEventPublisher {
    pub async fn new(channel: Channel) -> Result<Self, PublishError> {
        channel
            .exchange_declare(
                RECORD_INGESTED_EXCHANGE,
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
    async fn publish_record_ingested(&self, record: &RawRecord) -> Result<(), PublishError> {
        let payload =
            serde_json::to_vec(record).map_err(|e| PublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                RECORD_INGESTED_EXCHANGE,
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
