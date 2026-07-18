use super::*;
use axum::extract::Path;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryBacklogReader {
    pub depths: Mutex<Vec<QueueDepth>>,
}

#[async_trait]
impl BacklogReader for InMemoryBacklogReader {
    async fn queue_depths(&self) -> Result<Vec<QueueDepth>, BacklogError> {
        Ok(self.depths.lock().unwrap().clone())
    }
}

pub struct FailingBacklogReader;

#[async_trait]
impl BacklogReader for FailingBacklogReader {
    async fn queue_depths(&self) -> Result<Vec<QueueDepth>, BacklogError> {
        Err(BacklogError::Unreachable("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_reader_returns_configured_depths() {
    let reader = InMemoryBacklogReader::default();
    reader.depths.lock().unwrap().push(QueueDepth {
        stage: "ingest_to_normalize".to_string(),
        queue_name: "normalization-service.record.ingested".to_string(),
        messages: 5,
    });

    let depths = reader.queue_depths().await.unwrap();
    assert_eq!(depths.len(), 1);
    assert_eq!(depths[0].messages, 5);
}

#[tokio::test]
async fn failing_reader_returns_unreachable_error() {
    let reader = FailingBacklogReader;
    let err = reader.queue_depths().await.unwrap_err();
    assert!(matches!(err, BacklogError::Unreachable(_)));
}

async fn spawn_stub_management_api(queue_messages: HashMap<&'static str, u64>) -> String {
    async fn queues_handler(
        axum::extract::State(queue_messages): axum::extract::State<HashMap<&'static str, u64>>,
        Path((_vhost, queue_name)): Path<(String, String)>,
    ) -> axum::response::Response {
        match queue_messages.get(queue_name.as_str()) {
            Some(&messages) => Json(serde_json::json!({"messages": messages})).into_response(),
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }

    let app = Router::new()
        .route("/api/queues/:vhost/:queue_name", get(queues_handler))
        .with_state(queue_messages);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn rabbitmq_reader_returns_a_depth_per_pipeline_stage_against_a_real_server() {
    let mut queue_messages = HashMap::new();
    queue_messages.insert("normalization-service.record.ingested", 3);
    queue_messages.insert("analysis-service.record.normalized", 0);
    queue_messages.insert("trigger-engine.record.analyzed", 7);
    queue_messages.insert("action-executor.event.created", 1);
    let url = spawn_stub_management_api(queue_messages).await;

    let reader = RabbitMqManagementBacklogReader::new(reqwest::Client::new(), url, "/".to_string());
    let depths = reader.queue_depths().await.unwrap();

    assert_eq!(depths.len(), PIPELINE_QUEUES.len());
    let ingest_stage = depths.iter().find(|d| d.stage == "ingest_to_normalize").unwrap();
    assert_eq!(ingest_stage.messages, 3);
    let trigger_stage = depths.iter().find(|d| d.stage == "analyze_to_trigger").unwrap();
    assert_eq!(trigger_stage.messages, 7);
}

#[tokio::test]
async fn rabbitmq_reader_treats_an_undeclared_queue_as_zero_backlog() {
    let url = spawn_stub_management_api(HashMap::new()).await;

    let reader = RabbitMqManagementBacklogReader::new(reqwest::Client::new(), url, "/".to_string());
    let depths = reader.queue_depths().await.unwrap();

    assert!(depths.iter().all(|d| d.messages == 0));
}

#[tokio::test]
async fn rabbitmq_reader_returns_unreachable_when_server_is_down() {
    let reader = RabbitMqManagementBacklogReader::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1".to_string(),
        "/".to_string(),
    );
    let err = reader.queue_depths().await.unwrap_err();
    assert!(matches!(err, BacklogError::Unreachable(_)));
}
