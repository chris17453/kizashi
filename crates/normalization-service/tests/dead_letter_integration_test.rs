//! Integration test against real RabbitMQ (CLAUDE.md §2). Requires RABBITMQ_URL.

use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::BasicProperties;
use normalization_service::{DeadLetterManager, RabbitMqDeadLetterManager};
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

async fn declare_queue(channel: &lapin::Channel, name: &str) {
    channel
        .queue_declare(
            name,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
}

/// RabbitMQ's own queue statistics (the `message_count` a passive `queue_declare` returns) can
/// lag briefly behind a just-confirmed publish -- confirmed via double-`.await`ing
/// `basic_publish` isn't enough to make `count()` immediately consistent, this is RabbitMQ's own
/// eventual consistency, not a gap in this test or in `DeadLetterManager`. Poll instead of
/// asserting once.
async fn wait_for_count(manager: &impl DeadLetterManager, expected: u32) {
    for _ in 0..20 {
        if manager.count().await.unwrap() == expected {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(manager.count().await.unwrap(), expected, "count never converged");
}

#[tokio::test]
async fn count_reflects_messages_actually_published_to_a_real_queue() {
    let channel = test_channel().await;
    let main_queue = format!("test.dead-letter-main.{}", Uuid::new_v4());
    let dead_letter_queue = format!("test.dead-letter-dead.{}", Uuid::new_v4());
    declare_queue(&channel, &main_queue).await;
    declare_queue(&channel, &dead_letter_queue).await;

    channel
        .basic_publish(
            "",
            &dead_letter_queue,
            BasicPublishOptions::default(),
            b"poison message",
            BasicProperties::default(),
        )
        .await
        .unwrap()
        .await
        .unwrap();

    let manager =
        RabbitMqDeadLetterManager::new(channel, dead_letter_queue.clone(), main_queue.clone());

    wait_for_count(&manager, 1).await;
}

#[tokio::test]
async fn replay_oldest_moves_the_message_from_dead_letter_back_onto_the_main_queue() {
    let channel = test_channel().await;
    let main_queue = format!("test.dead-letter-main.{}", Uuid::new_v4());
    let dead_letter_queue = format!("test.dead-letter-dead.{}", Uuid::new_v4());
    declare_queue(&channel, &main_queue).await;
    declare_queue(&channel, &dead_letter_queue).await;

    channel
        .basic_publish(
            "",
            &dead_letter_queue,
            BasicPublishOptions::default(),
            b"poison message",
            BasicProperties::default(),
        )
        .await
        .unwrap()
        .await
        .unwrap();

    let manager = RabbitMqDeadLetterManager::new(
        channel.clone(),
        dead_letter_queue.clone(),
        main_queue.clone(),
    );

    let replayed = manager.replay_oldest().await.unwrap();
    assert!(replayed);
    wait_for_count(&manager, 0).await;

    let mut consumer = channel
        .basic_consume(
            &main_queue,
            "test-consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();
    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for the replayed message on the main queue")
        .unwrap()
        .unwrap();
    assert_eq!(delivery.data, b"poison message");
    delivery.ack(BasicAckOptions::default()).await.unwrap();
}

#[tokio::test]
async fn replay_oldest_returns_false_when_the_dead_letter_queue_is_empty() {
    let channel = test_channel().await;
    let main_queue = format!("test.dead-letter-main.{}", Uuid::new_v4());
    let dead_letter_queue = format!("test.dead-letter-dead.{}", Uuid::new_v4());
    declare_queue(&channel, &main_queue).await;
    declare_queue(&channel, &dead_letter_queue).await;

    let manager = RabbitMqDeadLetterManager::new(channel, dead_letter_queue, main_queue);

    assert!(!manager.replay_oldest().await.unwrap());
}
