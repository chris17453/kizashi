//! Full-stack integration test against real Postgres, ClickHouse, and RabbitMQ (CLAUDE.md §2).
//! Requires DATABASE_URL, CLICKHOUSE_URL, RABBITMQ_URL.

use common::{RawRecord, SourceType, ThresholdDirection, TriggerCondition, TriggerDefinition};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use serde_json::json;
use std::sync::Arc;
use trigger_engine::{
    process_analyzed_record, ClickHouseEventStore, PostgresSignalRepository,
    PostgresTriggerRepository, RabbitMqEventPublisher, TriggerDeps, EVENT_CREATED_EXCHANGE,
};
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "trigger_engine")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");
    pool
}

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
async fn a_firing_trigger_writes_to_clickhouse_and_publishes_event_created() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let pool = test_pool().await;
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let event_store =
        ClickHouseEventStore::new(reqwest::Client::new(), format!("{clickhouse_url}/"));
    event_store.ensure_schema().await.expect("failed to ensure clickhouse schema");

    let tenant_id = Uuid::new_v4();
    let trigger = TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "very negative sentiment".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::ThresholdOverWindow {
            field: "sentiment".to_string(),
            threshold: -0.5,
            direction: ThresholdDirection::Below,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    };
    sqlx::query(
        "INSERT INTO trigger_definitions (id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(trigger.id)
    .bind(trigger.tenant_id)
    .bind(&trigger.name)
    .bind(&trigger.event_type_match)
    .bind(serde_json::to_value(&trigger.condition).unwrap())
    .bind(trigger.window_seconds)
    .bind(serde_json::to_value(&trigger.actions).unwrap())
    .bind(trigger.enabled)
    .execute(&pool)
    .await
    .expect("failed to insert trigger definition");

    let publisher =
        RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange");
    let deps = TriggerDeps {
        trigger_repository: Arc::new(PostgresTriggerRepository::new(pool.clone())),
        signal_repository: Arc::new(PostgresSignalRepository::new(pool)),
        event_store: Arc::new(event_store),
        publisher: Arc::new(publisher),
    };

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
            EVENT_CREATED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "trigger-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let mut raw = RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        json!({"description": "printer on fire"}),
    );
    raw.normalized_payload = Some(json!({"entity_ref": "cust-integration-test"}));
    let record = common::AnalyzedRecord::new(raw, json!({"sentiment": -0.9}));

    let created = process_analyzed_record(&deps, &record).await.unwrap();
    assert_eq!(created, 1);

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for event.created")
        .expect("consumer stream ended unexpectedly")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("ack failed");

    let event: common::Event = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(event.tenant_id, tenant_id);
    assert_eq!(event.event_type, "sentiment");
    assert_eq!(event.group_key, "cust-integration-test");
}
