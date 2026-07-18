use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use normalization_service::{
    health_router, process_normalization, source_type_key, HttpRecordClient, NormalizationDeps,
    PostgresMappingRepository, RabbitMqEventPublisher, RECORD_INGESTED_EXCHANGE,
};
use std::sync::Arc;

const QUEUE_NAME: &str = "normalization-service.record.ingested";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let ingestion_service_url =
        std::env::var("INGESTION_SERVICE_URL").expect("INGESTION_SERVICE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

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

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let publish_channel = connection.create_channel().await.expect("failed to open channel");
    let consume_channel = connection.create_channel().await.expect("failed to open channel");

    let deps = NormalizationDeps {
        mapping_repository: Arc::new(PostgresMappingRepository::new(pool)),
        record_client: Arc::new(HttpRecordClient::new(
            reqwest::Client::new(),
            ingestion_service_url,
        )),
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
            RECORD_INGESTED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "normalization-service healthz listening");
    tokio::spawn(async move {
        axum::serve(listener, health_router()).await.expect("healthz server error");
    });

    let mut consumer = consume_channel
        .basic_consume(
            QUEUE_NAME,
            "normalization-service",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    tracing::info!("normalization-service consuming record.ingested");
    while let Some(delivery) = consumer.next().await {
        let delivery = match delivery {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "consumer delivery error");
                continue;
            }
        };

        let record: common::RawRecord = match serde_json::from_slice(&delivery.data) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "failed to deserialize record.ingested message, dropping");
                let _ = delivery.ack(BasicAckOptions::default()).await;
                continue;
            }
        };

        tracing::debug!(
            record_id = %record.id,
            source_type = %source_type_key(record.source_type),
            "processing"
        );

        match process_normalization(&deps, &record).await {
            Ok(_) => {
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
            Err(e) => {
                tracing::error!(record_id = %record.id, error = %e, "normalization failed, requeueing");
                let _ =
                    delivery.nack(BasicNackOptions { requeue: true, ..Default::default() }).await;
            }
        }
    }
}
