use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use std::sync::Arc;
use trigger_engine::{
    health_router, process_analyzed_record, ClickHouseEventStore, PostgresSignalRepository,
    PostgresTriggerRepository, RabbitMqEventPublisher, TriggerDeps, RECORD_ANALYZED_EXCHANGE,
};

const QUEUE_NAME: &str = "trigger-engine.record.analyzed";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let clickhouse_url = std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

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

    let event_store = ClickHouseEventStore::new(reqwest::Client::new(), clickhouse_url);
    event_store.ensure_schema().await.expect("failed to ensure clickhouse schema");

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let publish_channel = connection.create_channel().await.expect("failed to open channel");
    let consume_channel = connection.create_channel().await.expect("failed to open channel");

    let deps = TriggerDeps {
        trigger_repository: Arc::new(PostgresTriggerRepository::new(pool.clone())),
        signal_repository: Arc::new(PostgresSignalRepository::new(pool)),
        event_store: Arc::new(event_store),
        publisher: Arc::new(
            RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange"),
        ),
    };

    consume_channel
        .queue_declare(
            QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    consume_channel
        .queue_bind(
            QUEUE_NAME,
            RECORD_ANALYZED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "trigger-engine healthz listening");
    tokio::spawn(async move {
        axum::serve(listener, health_router()).await.expect("healthz server error");
    });

    let mut consumer = consume_channel
        .basic_consume(
            QUEUE_NAME,
            "trigger-engine",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    tracing::info!("trigger-engine consuming record.analyzed");
    while let Some(delivery) = consumer.next().await {
        let delivery = match delivery {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "consumer delivery error");
                continue;
            }
        };

        let record: common::AnalyzedRecord = match serde_json::from_slice(&delivery.data) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "failed to deserialize record.analyzed message, dropping");
                let _ = delivery.ack(BasicAckOptions::default()).await;
                continue;
            }
        };

        match process_analyzed_record(&deps, &record).await {
            Ok(created) => {
                if created > 0 {
                    tracing::info!(record_id = %record.record.id, events_created = created, "triggers fired");
                }
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
            Err(e) => {
                tracing::error!(record_id = %record.record.id, error = %e, "processing failed, requeueing");
                let _ =
                    delivery.nack(BasicNackOptions { requeue: true, ..Default::default() }).await;
            }
        }
    }
}
