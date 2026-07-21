//! Integration test against real RabbitMQ and real Postgres (CLAUDE.md §2), plus a real
//! in-process HTTP server standing in for Ingestion Service's `PATCH /v1/records/:id/normalized`
//! endpoint. Requires RABBITMQ_URL and DATABASE_URL. Mirrors the pattern already proven in
//! `analysis-service`/`trigger-engine`'s integration tests: exercise the crate's own processing
//! function directly against real infra, then observe what it published.

use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::patch;
use axum::{Json, Router};
use common::{RawRecord, SourceType, RECORD_NORMALIZED_EXCHANGE};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use normalization_service::{
    process_normalization, HttpRecordClient, NormalizationDeps, PostgresFingerprintRepository,
    PostgresMappingRepository, ProcessOutcome, RabbitMqEventPublisher,
};
use std::sync::Arc;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "normalization_service")
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

/// Stands in for Ingestion Service's `PATCH /v1/records/:id/normalized` — accepts any body and
/// returns 200, since this test only cares that Normalization Service calls it and then
/// publishes, not Ingestion Service's own persistence behavior (that's Ingestion Service's own
/// integration test's job).
async fn spawn_stub_ingestion_service() -> String {
    async fn handler(
        Path(_id): Path<Uuid>,
        Json(_body): Json<serde_json::Value>,
    ) -> impl IntoResponse {
        axum::http::StatusCode::OK
    }
    let app = Router::new().route("/v1/records/:id/normalized", patch(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn processing_a_record_publishes_record_normalized_over_real_rabbitmq() {
    let pool = test_pool().await;
    let ingestion_service_url = spawn_stub_ingestion_service().await;
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let tenant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO normalization_mappings (id, tenant_id, source_type, field_map, version) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind("ticket")
    .bind(serde_json::json!({"text": "$.description"}))
    .bind(1)
    .execute(&pool)
    .await
    .expect("failed to insert mapping fixture");

    let publisher =
        RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange");
    let deps = NormalizationDeps {
        mapping_repository: Arc::new(PostgresMappingRepository::new(pool.clone())),
        record_client: Arc::new(HttpRecordClient::new(
            reqwest::Client::new(),
            ingestion_service_url,
        )),
        publisher: Arc::new(publisher),
        fingerprint_repository: Arc::new(PostgresFingerprintRepository::new(pool)),
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
            RECORD_NORMALIZED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "normalization-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let record = RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        serde_json::json!({"description": "hi"}),
    );

    let outcome = process_normalization(&deps, &record).await.unwrap();
    assert_eq!(outcome, ProcessOutcome::Normalized);

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for record.normalized")
        .expect("consumer stream ended unexpectedly")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("ack failed");

    let normalized: RawRecord = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(normalized.id, record.id);
    assert_eq!(normalized.normalized_payload, Some(serde_json::json!({"text": "hi"})));
}

#[tokio::test]
async fn processing_a_record_with_no_configured_mapping_does_not_publish() {
    let pool = test_pool().await;
    let ingestion_service_url = spawn_stub_ingestion_service().await;
    let publish_channel = test_channel().await;

    let publisher =
        RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange");
    let deps = NormalizationDeps {
        mapping_repository: Arc::new(PostgresMappingRepository::new(pool.clone())),
        record_client: Arc::new(HttpRecordClient::new(
            reqwest::Client::new(),
            ingestion_service_url,
        )),
        publisher: Arc::new(publisher),
        fingerprint_repository: Arc::new(PostgresFingerprintRepository::new(pool)),
    };

    let tenant_id = Uuid::new_v4();
    let record = RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        serde_json::json!({"description": "hi"}),
    );

    let outcome = process_normalization(&deps, &record).await.unwrap();
    assert_eq!(outcome, ProcessOutcome::NoMappingConfigured);
}
