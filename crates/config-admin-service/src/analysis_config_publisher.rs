#[path = "analysis_config_publisher_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_config_publisher_test;

use async_trait::async_trait;
use common::{AnalysisConfig, ANALYSIS_CONFIG_CHANGED_EXCHANGE};
use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, ExchangeKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisConfigPublishError {
    #[error("message bus error: {0}")]
    Bus(String),
    #[error("failed to serialize analysis config for publish: {0}")]
    Serialization(String),
}

/// Publishes `analysis_config.changed` on every write (ADR-0019) so Analysis Service's own
/// copy of each tenant's AI prompt — the one it actually includes in Foundry/ML batch calls —
/// stays in sync with what operators author through this service's API/Console UI. Same
/// pattern as `TriggerPublisher` (ADR-0018).
#[async_trait]
pub trait AnalysisConfigPublisher: Send + Sync {
    async fn publish_analysis_config_changed(
        &self,
        config: &AnalysisConfig,
    ) -> Result<(), AnalysisConfigPublishError>;
}

pub struct RabbitMqAnalysisConfigPublisher {
    channel: Channel,
}

impl RabbitMqAnalysisConfigPublisher {
    pub async fn new(channel: Channel) -> Result<Self, AnalysisConfigPublishError> {
        channel
            .exchange_declare(
                ANALYSIS_CONFIG_CHANGED_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions { durable: true, ..Default::default() },
                FieldTable::default(),
            )
            .await
            .map_err(|e| AnalysisConfigPublishError::Bus(e.to_string()))?;
        Ok(Self { channel })
    }
}

#[async_trait]
impl AnalysisConfigPublisher for RabbitMqAnalysisConfigPublisher {
    async fn publish_analysis_config_changed(
        &self,
        config: &AnalysisConfig,
    ) -> Result<(), AnalysisConfigPublishError> {
        let payload = serde_json::to_vec(config)
            .map_err(|e| AnalysisConfigPublishError::Serialization(e.to_string()))?;
        self.channel
            .basic_publish(
                ANALYSIS_CONFIG_CHANGED_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| AnalysisConfigPublishError::Bus(e.to_string()))?
            .await
            .map_err(|e| AnalysisConfigPublishError::Bus(e.to_string()))?;
        Ok(())
    }
}
