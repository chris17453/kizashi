//! Integration test against real RabbitMQ (CLAUDE.md §2), proving ADR-0020's `agent.changed`
//! publish actually round-trips an `AgentChangeEvent` over the real bus. Requires RABBITMQ_URL.

use common::{Agent, AgentChangeEvent, AGENT_CHANGED_EXCHANGE};
use config_admin_service::{AgentPublisher, RabbitMqAgentPublisher};
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
/// `agent.changed` exchange, either queue can legitimately receive the *other* test's message
/// too. Loop-consuming until the exact expected event shows up (acking everything along the
/// way) makes the test robust to that interleaving instead of asserting on whatever arrives
/// first.
async fn wait_for_matching_event(
    consumer: &mut lapin::Consumer,
    expected: &AgentChangeEvent,
) -> AgentChangeEvent {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let delivery = tokio::time::timeout(remaining, consumer.next())
            .await
            .expect("timed out waiting for the expected agent.changed message")
            .expect("consumer stream ended")
            .expect("delivery error");
        delivery.ack(BasicAckOptions::default()).await.expect("failed to ack");

        let received: AgentChangeEvent = serde_json::from_slice(&delivery.data).unwrap();
        if &received == expected {
            return received;
        }
    }
}

#[tokio::test]
async fn publishing_an_upserted_agent_round_trips_over_real_rabbitmq() {
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqAgentPublisher::new(publish_channel).await.expect("failed to declare exchange");

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
            AGENT_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "agent-publisher-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let agent = Agent::new(
        Uuid::new_v4(),
        "zendesk",
        "integration-test-agent",
        serde_json::json!({"poll_interval_seconds": 60}),
    );
    let event = AgentChangeEvent::Upserted(agent);

    publisher.publish_agent_changed(&event).await.unwrap();

    let received = wait_for_matching_event(&mut consumer, &event).await;
    assert_eq!(received, event);
}

#[tokio::test]
async fn publishing_a_deleted_agent_round_trips_over_real_rabbitmq() {
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqAgentPublisher::new(publish_channel).await.expect("failed to declare exchange");

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
            AGENT_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "agent-publisher-integration-test-delete",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let event = AgentChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };
    publisher.publish_agent_changed(&event).await.unwrap();

    let received = wait_for_matching_event(&mut consumer, &event).await;
    assert_eq!(received, event);
}
