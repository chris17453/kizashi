//! End-to-end integration test against the real docker-compose stack (Postgres + RabbitMQ),
//! per CLAUDE.md §2: "not mocks... test against the real thing since we own it." Requires
//! DATABASE_URL and RABBITMQ_URL (see .env.example); CI provides both via service containers.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::StreamExt;
use ingestion_service::{
    build_router, IngestState, PostgresRawRecordRepository, RabbitMqEventPublisher,
    RECORD_INGESTED_EXCHANGE,
};
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use std::sync::Arc;
use tower::ServiceExt as _;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set to run ingest_integration_test");
    let pool = common::connect_with_schema(&database_url, "ingestion_service")
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
    let rabbitmq_url = std::env::var("RABBITMQ_URL")
        .expect("RABBITMQ_URL must be set to run ingest_integration_test");
    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    connection.create_channel().await.expect("failed to open channel")
}

#[tokio::test]
async fn posting_a_record_persists_it_and_publishes_record_ingested() {
    let pool = test_pool().await;
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange");

    // Bind a fresh exclusive queue to the fanout exchange so this test observes only its own
    // publish, independent of any other consumer.
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
            RECORD_INGESTED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "ingest-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let state = IngestState {
        repository: Arc::new(PostgresRawRecordRepository::new(pool.clone())),
        publisher: Arc::new(publisher),
    };
    let app = build_router(state);

    let tenant_id = Uuid::new_v4();
    let body = serde_json::json!({
        "connector_id": "zendesk",
        "source_type": "ticket",
        "tenant_id": tenant_id,
        "raw_payload": {"subject": "printer on fire"},
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/records")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let record_id: Uuid = serde_json::from_value(parsed["id"].clone()).unwrap();

    let row: (Uuid, String, Uuid) =
        sqlx::query_as("SELECT id, connector_id, tenant_id FROM raw_records WHERE id = $1")
            .bind(record_id)
            .fetch_one(&pool)
            .await
            .expect("row should exist after ingest");
    assert_eq!(row.0, record_id);
    assert_eq!(row.1, "zendesk");
    assert_eq!(row.2, tenant_id);

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for record.ingested message")
        .expect("consumer stream ended unexpectedly")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("ack failed");

    let published: serde_json::Value = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(published["id"], serde_json::json!(record_id));
    assert_eq!(published["tenant_id"], serde_json::json!(tenant_id));
}
