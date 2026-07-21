#[path = "dead_letter_test.rs"]
#[cfg(test)]
pub(crate) mod dead_letter_test;

use async_trait::async_trait;
use lapin::options::{BasicAckOptions, BasicGetOptions, BasicPublishOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeadLetterError {
    #[error("rabbitmq backend error: {0}")]
    Backend(String),
}

/// Operator-facing visibility/recovery for `retry.rs`'s dead-letter queue -- previously a
/// message that exceeded `MAX_RETRIES` vanished into a queue with no way to see it existed or
/// get it back into the main pipeline once the underlying cause (a bad tenant config, a
/// transient downstream outage) was fixed. `replay_oldest` resets the retry-count header on
/// republish, so a replayed message gets a full fresh `MAX_RETRIES` budget rather than
/// dead-lettering again immediately.
#[async_trait]
pub trait DeadLetterManager: Send + Sync {
    async fn count(&self) -> Result<u32, DeadLetterError>;
    async fn replay_oldest(&self) -> Result<bool, DeadLetterError>;
}

pub struct RabbitMqDeadLetterManager {
    channel: lapin::Channel,
    dead_letter_queue: String,
    main_queue: String,
}

impl RabbitMqDeadLetterManager {
    pub fn new(channel: lapin::Channel, dead_letter_queue: String, main_queue: String) -> Self {
        Self { channel, dead_letter_queue, main_queue }
    }
}

#[async_trait]
impl DeadLetterManager for RabbitMqDeadLetterManager {
    async fn count(&self) -> Result<u32, DeadLetterError> {
        let queue = self
            .channel
            .queue_declare(
                &self.dead_letter_queue,
                QueueDeclareOptions { durable: true, passive: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| DeadLetterError::Backend(e.to_string()))?;
        Ok(queue.message_count())
    }

    async fn replay_oldest(&self) -> Result<bool, DeadLetterError> {
        let Some(message) = self
            .channel
            .basic_get(&self.dead_letter_queue, BasicGetOptions::default())
            .await
            .map_err(|e| DeadLetterError::Backend(e.to_string()))?
        else {
            return Ok(false);
        };

        // Strip every header (including the retry-count one) rather than just resetting it to
        // 0, so a replayed message is indistinguishable from a brand-new one downstream.
        let reset_properties =
            message.delivery.properties.clone().with_headers(FieldTable::default());
        self.channel
            .basic_publish(
                "",
                &self.main_queue,
                BasicPublishOptions::default(),
                &message.delivery.data,
                reset_properties,
            )
            .await
            .map_err(|e| DeadLetterError::Backend(e.to_string()))?
            .await
            .map_err(|e| DeadLetterError::Backend(e.to_string()))?;

        message
            .delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|e| DeadLetterError::Backend(e.to_string()))?;

        Ok(true)
    }
}
