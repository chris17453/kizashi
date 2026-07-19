use super::*;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryBacklogClient {
    pub depths: Mutex<Vec<QueueDepthSummary>>,
}

#[async_trait]
impl BacklogClient for InMemoryBacklogClient {
    async fn queue_depths(&self) -> Result<Vec<QueueDepthSummary>, BacklogClientError> {
        Ok(self.depths.lock().unwrap().clone())
    }
}

pub struct FailingBacklogClient;

#[async_trait]
impl BacklogClient for FailingBacklogClient {
    async fn queue_depths(&self) -> Result<Vec<QueueDepthSummary>, BacklogClientError> {
        Err(BacklogClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn handler() -> axum::response::Response {
        Json(serde_json::json!([
            {"stage": "ingest_to_normalize", "queue_name": "normalization-service.record.ingested", "messages": 3}
        ]))
        .into_response()
    }
    let app = Router::new().route("/v1/backlog", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_reads_backlog_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpBacklogClient::new(reqwest::Client::new(), url);

    let depths = client.queue_depths().await.unwrap();

    assert_eq!(depths.len(), 1);
    assert_eq!(depths[0].stage, "ingest_to_normalize");
    assert_eq!(depths[0].messages, 3);
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpBacklogClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.queue_depths().await.unwrap_err();
    assert!(matches!(err, BacklogClientError::Unreachable(_)));
}
