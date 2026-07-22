use common::{bus::RECORD_NORMALIZED_EXCHANGE, RawRecord};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use std::sync::Arc;

mod mapping_engine;

use ontology_service::build_router;

const QUEUE_NAME: &str = "ontology-service.record.normalized";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = common::connect_with_schema(&database_url, "ontology_service")
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
    let consume_channel = connection.create_channel().await.expect("failed to open channel");

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
            RECORD_NORMALIZED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let engine = Arc::new(mapping_engine::OntologyMappingEngine::new(pool.clone()));

    let mut consumer = consume_channel
        .basic_consume(
            QUEUE_NAME,
            "ontology-service",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    tracing::info!("ontology-service consuming record.normalized");

    let engine_clone = engine.clone();
    tokio::spawn(async move {
        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(error = %e, "consumer delivery error");
                    continue;
                }
            };

            let record: RawRecord = match serde_json::from_slice(&delivery.data) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "failed to deserialize record.normalized message, dropping");
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                    continue;
                }
            };

            if let Err(e) = engine_clone.process_record(record).await {
                tracing::error!(error = %e, "failed to process record");
                // basic nack could be implemented, keeping it simple for now
            }
            let _ = delivery.ack(BasicAckOptions::default()).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "ontology-service listening");

    // Pass pool to router so it can be used for API queries
    axum::serve(listener, build_router(pool)).await.expect("server error");
}
