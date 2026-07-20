//! Integration test against real RabbitMQ (CLAUDE.md §2), proving ADR-0020's `sensor.changed`
//! publish actually round-trips an `SensorChangeEvent` over the real bus. Requires RABBITMQ_URL.

use common::{Sensor, SensorChangeEvent, SENSOR_CHANGED_EXCHANGE};
use config_admin_service::{RabbitMqSensorPublisher, SensorPublisher};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
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

/// The fanout exchange broadcasts every message to every queue bound to it — since this test
/// binary's two tests can run concurrently and both bind their own queue to the same real
/// `sensor.changed` exchange, either queue can legitimately receive the *other* test's message
/// too. Loop-consuming until the exact expected event shows up (acking everything along the
/// way) makes the test robust to that interleaving instead of asserting on whatever arrives
/// first.
async fn wait_for_matching_event(
    consumer: &mut lapin::Consumer,
    expected: &SensorChangeEvent,
) -> SensorChangeEvent {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let delivery = tokio::time::timeout(remaining, consumer.next())
            .await
            .expect("timed out waiting for the expected sensor.changed message")
            .expect("consumer stream ended")
            .expect("delivery error");
        delivery.ack(BasicAckOptions::default()).await.expect("failed to ack");

        let received: SensorChangeEvent = serde_json::from_slice(&delivery.data).unwrap();
        if &received == expected {
            return received;
        }
    }
}

#[tokio::test]
async fn publishing_an_upserted_sensor_round_trips_over_real_rabbitmq() {
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqSensorPublisher::new(publish_channel).await.expect("failed to declare exchange");

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
            SENSOR_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "sensor-publisher-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let sensor = Sensor::new(
        Uuid::new_v4(),
        "zendesk",
        "integration-test-sensor",
        serde_json::json!({"poll_interval_seconds": 60}),
    );
    let event = SensorChangeEvent::Upserted(sensor);

    publisher.publish_sensor_changed(&event).await.unwrap();

    let received = wait_for_matching_event(&mut consumer, &event).await;
    assert_eq!(received, event);
}

#[tokio::test]
async fn publishing_a_deleted_sensor_round_trips_over_real_rabbitmq() {
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqSensorPublisher::new(publish_channel).await.expect("failed to declare exchange");

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
            SENSOR_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "sensor-publisher-integration-test-delete",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let event = SensorChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };
    publisher.publish_sensor_changed(&event).await.unwrap();

    let received = wait_for_matching_event(&mut consumer, &event).await;
    assert_eq!(received, event);
}
