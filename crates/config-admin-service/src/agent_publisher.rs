#[path = "agent_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod agent_publisher_test;

use async_trait::async_trait;
use common::{AgentChangeEvent, AGENT_CHANGED_EXCHANGE};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentPublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize agent change for publish: {0}")]
    Serialization(String),
}

/// Publishes `agent.changed` on every Agent create/update/delete (ADR-0020) so Agent
/// Scheduler's own copy of the registry — the one it walks to decide what's due to poll —
/// stays in sync. Same pattern as `TriggerPublisher`/`AnalysisConfigPublisher`.
#[async_trait]
pub trait AgentPublisher: Send + Sync {
    async fn publish_agent_changed(
        &self,
        event: &AgentChangeEvent,
    ) -> Result<(), AgentPublishError>;
}

pub struct RabbitMqAgentPublisher {
    channel: Channel,
}

impl RabbitMqAgentPublisher {
    pub async fn new(channel: Channel) -> Result<Self, AgentPublishError> {
        channel
            .exchange_declare(
                AGENT_CHANGED_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions { durable: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| AgentPublishError::Bus(e.to_string()))?;
        Ok(Self { channel })
    }
}

#[async_trait]
impl AgentPublisher for RabbitMqAgentPublisher {
    async fn publish_agent_changed(
        &self,
        event: &AgentChangeEvent,
    ) -> Result<(), AgentPublishError> {
        let payload = serde_json::to_vec(event)
            .map_err(|e| AgentPublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                AGENT_CHANGED_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| AgentPublishError::Bus(e.to_string()))?
            .await
            .map_err(|e| AgentPublishError::Bus(e.to_string()))?;
        Ok(())
    }
}
