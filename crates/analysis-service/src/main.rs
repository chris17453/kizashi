use analysis_service::{
    health_router, process_batch, AnalysisDeps, FoundryAnalysisClient, RabbitMqEventPublisher,
    RECORD_NORMALIZED_EXCHANGE,
};
use futures_util::StreamExt;
use lapin::message::Delivery;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const QUEUE_NAME: &str = "analysis-service.record.normalized";

fn batch_size() -> usize {
    std::env::var("ANALYSIS_BATCH_SIZE").ok().and_then(|v| v.parse().ok()).unwrap_or(20)
}

fn batch_max_wait() -> Duration {
    let ms = std::env::var("ANALYSIS_BATCH_MAX_WAIT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    Duration::from_millis(ms)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let foundry_endpoint =
        std::env::var("AZURE_AI_FOUNDRY_ENDPOINT").expect("AZURE_AI_FOUNDRY_ENDPOINT must be set");
    let foundry_api_key =
        std::env::var("AZURE_AI_FOUNDRY_API_KEY").expect("AZURE_AI_FOUNDRY_API_KEY must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let publish_channel = connection.create_channel().await.expect("failed to open channel");
    let consume_channel = connection.create_channel().await.expect("failed to open channel");

    let deps = AnalysisDeps {
        analysis_client: Arc::new(FoundryAnalysisClient::new(
            reqwest::Client::new(),
            foundry_endpoint,
            foundry_api_key,
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
            RECORD_NORMALIZED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "analysis-service healthz listening");
    tokio::spawn(async move {
        axum::serve(listener, health_router()).await.expect("healthz server error");
    });

    let mut consumer = consume_channel
        .basic_consume(
            QUEUE_NAME,
            "analysis-service",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let max_batch_size = batch_size();
    let max_wait = batch_max_wait();
    tracing::info!(max_batch_size, ?max_wait, "analysis-service consuming record.normalized");

    loop {
        let mut buffer: Vec<(Delivery, common::RawRecord)> = Vec::new();
        let deadline = tokio::time::sleep(max_wait);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                maybe_delivery = consumer.next() => {
                    match maybe_delivery {
                        Some(Ok(delivery)) => {
                            match serde_json::from_slice::<common::RawRecord>(&delivery.data) {
                                Ok(record) => buffer.push((delivery, record)),
                                Err(e) => {
                                    tracing::error!(error = %e, "failed to deserialize record.normalized message, dropping");
                                    let _ = delivery.ack(BasicAckOptions::default()).await;
                                }
                            }
                            if buffer.len() >= max_batch_size {
                                break;
                            }
                        }
                        Some(Err(e)) => tracing::error!(error = %e, "consumer delivery error"),
                        None => return,
                    }
                }
                _ = &mut deadline => break,
            }
        }

        if buffer.is_empty() {
            continue;
        }

        let mut groups: HashMap<Uuid, Vec<(Delivery, common::RawRecord)>> = HashMap::new();
        for (delivery, record) in buffer {
            groups.entry(record.tenant_id).or_default().push((delivery, record));
        }

        for (tenant_id, entries) in groups {
            let (deliveries, records): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
            match process_batch(&deps, tenant_id, records).await {
                Ok(_) => {
                    for delivery in deliveries {
                        let _ = delivery.ack(BasicAckOptions::default()).await;
                    }
                }
                Err(e) => {
                    tracing::error!(%tenant_id, error = %e, "batch analysis failed, requeueing");
                    for delivery in deliveries {
                        let _ = delivery
                            .nack(BasicNackOptions { requeue: true, ..Default::default() })
                            .await;
                    }
                }
            }
        }
    }
}
