#[cfg(test)]
#[path = "outbox_publisher_test.rs"]
mod outbox_publisher_test;

use crate::OutboxRepository;
use chrono::Duration;
use lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, ExchangeKind,
};

pub async fn publish_once(
    repository: &dyn OutboxRepository,
    channel: &Channel,
) -> Result<usize, String> {
    channel
        .exchange_declare(
            common::PIPELINE_EXECUTION_EXCHANGE,
            ExchangeKind::Topic,
            ExchangeDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let messages =
        repository.lease(50, Duration::seconds(30)).await.map_err(|error| error.to_string())?;
    let mut published = 0;
    for message in messages {
        let payload = serde_json::to_vec(&message.payload).map_err(|error| error.to_string())?;
        match channel
            .basic_publish(
                common::PIPELINE_EXECUTION_EXCHANGE,
                &message.event_type,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default(),
            )
            .await
        {
            Ok(confirm) => match confirm.await {
                Ok(_) => {
                    repository
                        .mark_published(message.id)
                        .await
                        .map_err(|error| error.to_string())?;
                    published += 1;
                }
                Err(error) => repository
                    .release(message.id, &error.to_string())
                    .await
                    .map_err(|error| error.to_string())?,
            },
            Err(error) => repository
                .release(message.id, &error.to_string())
                .await
                .map_err(|error| error.to_string())?,
        }
    }
    Ok(published)
}
