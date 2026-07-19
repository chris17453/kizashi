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

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for agent.changed")
        .expect("consumer stream ended")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("failed to ack");

    let received: AgentChangeEvent = serde_json::from_slice(&delivery.data).unwrap();
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

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for agent.changed")
        .expect("consumer stream ended")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("failed to ack");

    let received: AgentChangeEvent = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(received, event);
}
