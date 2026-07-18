use super::*;
use axum::response::IntoResponse;
use axum::routing::patch;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryRecordClient {
    pub updates: Mutex<Vec<(Uuid, serde_json::Value)>>,
}

#[async_trait]
impl RecordClient for InMemoryRecordClient {
    async fn update_normalized_payload(
        &self,
        record_id: Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<(), RecordClientError> {
        self.updates.lock().unwrap().push((record_id, normalized_payload.clone()));
        Ok(())
    }
}

pub struct FailingRecordClient;

#[async_trait]
impl RecordClient for FailingRecordClient {
    async fn update_normalized_payload(
        &self,
        _record_id: Uuid,
        _normalized_payload: &serde_json::Value,
    ) -> Result<(), RecordClientError> {
        Err(RecordClientError::Unreachable("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_client_records_updates() {
    let client = InMemoryRecordClient::default();
    let record_id = Uuid::new_v4();
    let payload = serde_json::json!({"text": "hi"});

    client.update_normalized_payload(record_id, &payload).await.unwrap();

    let updates = client.updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0], (record_id, payload));
}

#[tokio::test]
async fn failing_client_returns_unreachable_error() {
    let client = FailingRecordClient;
    let err =
        client.update_normalized_payload(Uuid::new_v4(), &serde_json::json!({})).await.unwrap_err();
    assert!(matches!(err, RecordClientError::Unreachable(_)));
}

async fn spawn_stub_server(status: axum::http::StatusCode) -> String {
    async fn ok_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::NO_CONTENT
    }
    async fn error_handler() -> axum::response::Response {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }

    let app = if status.is_success() {
        Router::new().route("/v1/records/:id/normalized", patch(ok_handler))
    } else {
        Router::new().route("/v1/records/:id/normalized", patch(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_succeeds_against_a_real_server_returning_204() {
    let url = spawn_stub_server(axum::http::StatusCode::NO_CONTENT).await;
    let client = HttpRecordClient::new(reqwest::Client::new(), url);

    client
        .update_normalized_payload(Uuid::new_v4(), &serde_json::json!({"text": "hi"}))
        .await
        .unwrap();
}

#[tokio::test]
async fn http_client_returns_rejected_when_server_errors() {
    let url = spawn_stub_server(axum::http::StatusCode::INTERNAL_SERVER_ERROR).await;
    let client = HttpRecordClient::new(reqwest::Client::new(), url);

    let err =
        client.update_normalized_payload(Uuid::new_v4(), &serde_json::json!({})).await.unwrap_err();
    assert!(matches!(err, RecordClientError::Rejected(500)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpRecordClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err =
        client.update_normalized_payload(Uuid::new_v4(), &serde_json::json!({})).await.unwrap_err();
    assert!(matches!(err, RecordClientError::Unreachable(_)));
}
