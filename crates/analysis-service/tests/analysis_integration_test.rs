//! Integration test against real RabbitMQ (CLAUDE.md §2) plus a real in-process HTTP server
//! standing in for Azure AI Foundry. Requires RABBITMQ_URL.

use analysis_service::{
    process_batch, AnalysisConfigRepository, AnalysisConfigRepositoryError, AnalysisDeps,
    FoundryAnalysisClient, RabbitMqEventPublisher, RECORD_ANALYZED_EXCHANGE,
};
use async_trait::async_trait;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use common::{AnalysisConfig, RawRecord, SourceType};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// No config ever configured — this integration test only proves the RabbitMQ publish path,
/// not analysis-config sync (see `analysis_config_repository_test.rs` for that).
struct NoAnalysisConfig;

#[async_trait]
impl AnalysisConfigRepository for NoAnalysisConfig {
    async fn get(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<AnalysisConfig>, AnalysisConfigRepositoryError> {
        Ok(None)
    }

    async fn upsert(&self, _config: AnalysisConfig) -> Result<(), AnalysisConfigRepositoryError> {
        Ok(())
    }
}

async fn spawn_stub_foundry() -> String {
    async fn handler(Json(body): Json<serde_json::Value>) -> axum::response::Response {
        let count = body["inputs"].as_array().map(|a| a.len()).unwrap_or(0);
        let results: Vec<serde_json::Value> =
            (0..count).map(|_| json!({"sentiment": -0.9})).collect();
        Json(json!({"results": results})).into_response()
    }
    let app = Router::new().route("/analyze", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/analyze")
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
async fn processing_a_batch_publishes_record_analyzed_over_real_rabbitmq() {
    let foundry_endpoint = spawn_stub_foundry().await;
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let publisher =
        RabbitMqEventPublisher::new(publish_channel).await.expect("failed to declare exchange");
    let deps = AnalysisDeps {
        analysis_client: Arc::new(FoundryAnalysisClient::new(
            reqwest::Client::new(),
            foundry_endpoint,
            "test-key".to_string(),
        )),
        publisher: Arc::new(publisher),
        analysis_config_repository: Arc::new(NoAnalysisConfig),
        http_client: reqwest::Client::new(),
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
            RECORD_ANALYZED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "analysis-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let tenant_id = Uuid::new_v4();
    let mut record =
        RawRecord::new("zendesk", SourceType::Ticket, tenant_id, json!({"description": "hi"}));
    record.normalized_payload = Some(json!({"text": "hi"}));

    let published = process_batch(&deps, tenant_id, vec![record.clone()]).await.unwrap();
    assert_eq!(published, 1);

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for record.analyzed")
        .expect("consumer stream ended unexpectedly")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("ack failed");

    let analyzed: common::AnalyzedRecord = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(analyzed.record.id, record.id);
    assert_eq!(analyzed.analysis, json!({"sentiment": -0.9}));
}
