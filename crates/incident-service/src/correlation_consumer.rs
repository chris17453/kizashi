#[path = "correlation_consumer_test.rs"]
#[cfg(test)]
mod correlation_consumer_test;

use crate::incident_repository::EventCorrelationIdentity;
use common::Event;
use common::EVENT_CREATED_EXCHANGE;
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties};
use std::sync::Arc;
use uuid::Uuid;

const QUEUE_NAME: &str = "incident-service.event.created.correlation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrelationOutcome {
    Ignored,
    Ambiguous,
    Linked(Uuid),
    PartialLinked(Uuid),
}

pub async fn correlate_event(
    repository: &Arc<dyn crate::IncidentRepository>,
    event: &Event,
) -> Result<CorrelationOutcome, crate::IncidentRepositoryError> {
    let group_key = event.group_key.trim();
    if group_key.is_empty() {
        return Ok(CorrelationOutcome::Ignored);
    }
    let candidates =
        repository.active_incident_ids_for_group_key(event.tenant_id, group_key).await?;
    if candidates.len() > 1 {
        return Ok(CorrelationOutcome::Ambiguous);
    }
    if let Some(incident_id) = candidates.first().copied() {
        repository
            .link_event_with_identity(
                event.tenant_id,
                incident_id,
                event.id,
                Some(group_key),
                EventCorrelationIdentity {
                    event_type: &event.event_type,
                    entity_ref: &event.entity_ref,
                },
                "event-correlation",
            )
            .await?;
        return Ok(CorrelationOutcome::Linked(incident_id));
    }
    let identity =
        EventCorrelationIdentity { event_type: &event.event_type, entity_ref: &event.entity_ref };
    let partial =
        repository.active_incident_ids_for_event_identity(event.tenant_id, identity).await?;
    let Some(incident_id) = (partial.len() == 1).then(|| partial[0]) else {
        return Ok(if partial.is_empty() {
            CorrelationOutcome::Ignored
        } else {
            CorrelationOutcome::Ambiguous
        });
    };
    repository
        .link_event_with_identity(
            event.tenant_id,
            incident_id,
            event.id,
            Some(group_key),
            identity,
            "event-partial-correlation",
        )
        .await?;
    Ok(CorrelationOutcome::PartialLinked(incident_id))
}

pub async fn run(rabbitmq_url: String, repository: Arc<dyn crate::IncidentRepository>) {
    loop {
        match consume_once(&rabbitmq_url, repository.clone()).await {
            Ok(()) => tracing::warn!("incident correlation consumer stopped; reconnecting"),
            Err(error) => {
                tracing::error!(%error, "incident correlation consumer failed; reconnecting")
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn consume_once(
    rabbitmq_url: &str,
    repository: Arc<dyn crate::IncidentRepository>,
) -> Result<(), String> {
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection.create_channel().await.map_err(|error| error.to_string())?;
    channel
        .queue_declare(
            QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            QUEUE_NAME,
            EVENT_CREATED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut consumer = channel
        .basic_consume(
            QUEUE_NAME,
            "incident-correlation",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    while let Some(delivery) = consumer.next().await {
        let delivery = delivery.map_err(|error| error.to_string())?;
        let event = match serde_json::from_slice::<Event>(&delivery.data) {
            Ok(event) => event,
            Err(error) => {
                tracing::error!(%error, "invalid event.created message for incident correlation");
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|error| error.to_string())?;
                continue;
            }
        };
        match correlate_event(&repository, &event).await {
            Ok(
                CorrelationOutcome::Linked(incident_id)
                | CorrelationOutcome::PartialLinked(incident_id),
            ) => {
                tracing::info!(%incident_id, event_id = %event.id, "auto-correlated event into active incident");
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Ok(CorrelationOutcome::Ignored | CorrelationOutcome::Ambiguous) => {
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => {
                tracing::error!(%error, event_id = %event.id, "incident correlation lookup failed");
                delivery
                    .nack(BasicNackOptions { requeue: true, ..Default::default() })
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}
