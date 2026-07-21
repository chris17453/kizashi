use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::FieldTable;
use normalization_service::{
    dead_letter_router, health_router, process_normalization, retry_count, should_dead_letter,
    source_type_key, with_incremented_retry_count, DeadLetterState, HttpRecordClient,
    NormalizationDeps, PostgresFingerprintRepository, PostgresMappingRepository,
    RabbitMqDeadLetterManager, RabbitMqEventPublisher, MAPPING_CHANGED_EXCHANGE,
    RECORD_INGESTED_EXCHANGE,
};
use std::sync::Arc;

const QUEUE_NAME: &str = "normalization-service.record.ingested";
const DEAD_LETTER_QUEUE_NAME: &str = "normalization-service.record.ingested.dead";
const MAPPING_CHANGED_QUEUE_NAME: &str = "normalization-service.mapping.changed";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let ingestion_service_url =
        std::env::var("INGESTION_SERVICE_URL").expect("INGESTION_SERVICE_URL must be set");
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
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
    let mapping_changed_channel =
        connection.create_channel().await.expect("failed to open channel");
    let dead_letter_channel = connection.create_channel().await.expect("failed to open channel");

    let deps = NormalizationDeps {
        mapping_repository: Arc::new(PostgresMappingRepository::new(pool.clone())),
        record_client: Arc::new(HttpRecordClient::new(
            reqwest::Client::new(),
            ingestion_service_url,
        )),
        publisher: Arc::new(
            RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange"),
        ),
        fingerprint_repository: Arc::new(PostgresFingerprintRepository::new(pool)),
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
        .queue_declare(
            DEAD_LETTER_QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare dead-letter queue");
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

    mapping_changed_channel
        .exchange_declare(
            MAPPING_CHANGED_EXCHANGE,
            lapin::ExchangeKind::Fanout,
            lapin::options::ExchangeDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare mapping.changed exchange");
    mapping_changed_channel
        .queue_declare(
            MAPPING_CHANGED_QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    mapping_changed_channel
        .queue_bind(
            MAPPING_CHANGED_QUEUE_NAME,
            MAPPING_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let dead_letter_state = DeadLetterState {
        dead_letter_manager: Arc::new(RabbitMqDeadLetterManager::new(
            dead_letter_channel,
            DEAD_LETTER_QUEUE_NAME.to_string(),
            QUEUE_NAME.to_string(),
        )),
        internal_secret,
    };
    let app = health_router().merge(dead_letter_router(dead_letter_state));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "normalization-service healthz listening");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("healthz server error");
    });

    let mut mapping_changed_consumer = mapping_changed_channel
        .basic_consume(
            MAPPING_CHANGED_QUEUE_NAME,
            "normalization-service.mapping.changed",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");
    let mapping_repository = deps.mapping_repository.clone();
    tokio::spawn(async move {
        tracing::info!("normalization-service consuming mapping.changed");
        while let Some(delivery) = mapping_changed_consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(error = %e, "mapping.changed consumer delivery error");
                    continue;
                }
            };

            let event: common::MappingChangeEvent = match serde_json::from_slice(&delivery.data) {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!(error = %e, "failed to deserialize mapping.changed message, dropping");
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                    continue;
                }
            };

            let mapping_id = match &event {
                common::MappingChangeEvent::Upserted(mapping) => mapping.id,
                common::MappingChangeEvent::Deleted { id, .. } => *id,
            };
            let result = match &event {
                common::MappingChangeEvent::Upserted(mapping) => {
                    mapping_repository.upsert(mapping.clone()).await
                }
                common::MappingChangeEvent::Deleted { id, .. } => {
                    mapping_repository.delete(*id).await
                }
            };

            match result {
                Ok(()) => {
                    tracing::info!(mapping_id = %mapping_id, "synced mapping.changed");
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                }
                Err(e) => {
                    tracing::error!(mapping_id = %mapping_id, error = %e, "failed to sync mapping, requeueing");
                    let _ = delivery
                        .nack(BasicNackOptions { requeue: true, ..Default::default() })
                        .await;
                }
            }
        }
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
                let headers = delivery.properties.headers().as_ref();
                if should_dead_letter(headers) {
                    tracing::error!(
                        record_id = %record.id,
                        error = %e,
                        retries = retry_count(headers),
                        "message exceeded max retries, dead-lettering"
                    );
                    let _ = consume_channel
                        .basic_publish(
                            "",
                            DEAD_LETTER_QUEUE_NAME,
                            BasicPublishOptions::default(),
                            &delivery.data,
                            delivery.properties.clone(),
                        )
                        .await;
                } else {
                    tracing::error!(record_id = %record.id, error = %e, "normalization failed, requeueing");
                    let next_headers = with_incremented_retry_count(headers);
                    let _ = consume_channel
                        .basic_publish(
                            "",
                            QUEUE_NAME,
                            BasicPublishOptions::default(),
                            &delivery.data,
                            delivery.properties.clone().with_headers(next_headers),
                        )
                        .await;
                }
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
        }
    }
}
