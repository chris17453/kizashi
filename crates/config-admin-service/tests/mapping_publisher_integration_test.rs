//! Integration test against real RabbitMQ (CLAUDE.md §2), proving `mapping.changed` actually
//! round-trips a `NormalizationMapping` over the real bus — the same closure ADR-0018 already
//! proved for `trigger.changed`, applied to the sibling gap ADR-0010 left open for
//! normalization mappings. Requires RABBITMQ_URL.

use common::{NormalizationMapping, MAPPING_CHANGED_EXCHANGE};
use config_admin_service::{MappingPublisher, RabbitMqMappingPublisher};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use std::collections::BTreeMap;
use uuid::Uuid;

async fn test_channel() -> lapin::Channel {
    let rabbitmq_url =
        std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set to run this test");
    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    connection.create_channel().await.expect("failed to open channel")
}

#[tokio::test]
async fn publishing_a_mapping_change_round_trips_over_real_rabbitmq() {
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqMappingPublisher::new(publish_channel).await.expect("failed to declare exchange");

    let queue = consume_channel
        .queue_declare(
            "",
            QueueDeclareOptions { exclusive: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    consume_channel
        .queue_bind(
            queue.name().as_str(),
            MAPPING_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "mapping-publisher-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    let mapping = NormalizationMapping::new(Uuid::new_v4(), "integration-test-ticket", field_map);

    publisher.publish_mapping_changed(&mapping).await.unwrap();

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for mapping.changed")
        .expect("consumer stream ended")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("failed to ack");

    let received: NormalizationMapping = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(received, mapping);
}
