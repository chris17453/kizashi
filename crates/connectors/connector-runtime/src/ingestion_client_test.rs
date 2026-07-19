use super::*;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use common::SourceType;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryIngestionClient {
    pub ingested: Mutex<Vec<RawRecord>>,
}

#[async_trait]
impl IngestionClient for InMemoryIngestionClient {
    async fn ingest(&self, record: &RawRecord) -> Result<(), IngestionClientError> {
        self.ingested.lock().unwrap().push(record.clone());
        Ok(())
    }
}

pub struct FailingIngestionClient;

#[async_trait]
impl IngestionClient for FailingIngestionClient {
    async fn ingest(&self, _record: &RawRecord) -> Result<(), IngestionClientError> {
        Err(IngestionClientError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_record() -> RawRecord {
    RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), serde_json::json!({"a": 1}))
}

#[tokio::test]
async fn in_memory_client_records_ingested_records() {
    let client = InMemoryIngestionClient::default();
    let record = sample_record();

    client.ingest(&record).await.unwrap();

    assert_eq!(*client.ingested.lock().unwrap(), vec![record]);
}

#[tokio::test]
async fn failing_client_returns_unreachable_error() {
    let client = FailingIngestionClient;
    let err = client.ingest(&sample_record()).await.unwrap_err();
    assert!(matches!(err, IngestionClientError::Unreachable(_)));
}

async fn spawn_stub_server(
    status: axum::http::StatusCode,
    expected_api_key: &'static str,
) -> String {
    async fn handler(
        axum::extract::State(expected_api_key): axum::extract::State<&'static str>,
        headers: axum::http::HeaderMap,
    ) -> axum::response::Response {
        let key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
        if key == Some(expected_api_key) {
            axum::http::StatusCode::CREATED.into_response()
        } else {
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
    async fn error_handler() -> axum::response::Response {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }

    let app = if status.is_success() {
        Router::new().route("/v1/ingest", post(handler)).with_state(expected_api_key)
    } else {
        Router::new().route("/v1/ingest", post(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_succeeds_against_a_real_server_with_the_correct_api_key() {
    let url = spawn_stub_server(axum::http::StatusCode::CREATED, "correct-key").await;
    let client = HttpIngestionClient::new(reqwest::Client::new(), url, "correct-key".to_string());

    client.ingest(&sample_record()).await.unwrap();
}

#[tokio::test]
async fn http_client_is_rejected_with_the_wrong_api_key() {
    let url = spawn_stub_server(axum::http::StatusCode::CREATED, "correct-key").await;
    let client = HttpIngestionClient::new(reqwest::Client::new(), url, "wrong-key".to_string());

    let err = client.ingest(&sample_record()).await.unwrap_err();
    assert!(matches!(err, IngestionClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_sends_the_records_external_id_in_the_request_body() {
    async fn handler(axum::Json(body): axum::Json<serde_json::Value>) -> axum::http::StatusCode {
        assert_eq!(body["external_id"], serde_json::json!("message-id-123"));
        axum::http::StatusCode::CREATED
    }
    let app = Router::new().route("/v1/ingest", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = HttpIngestionClient::new(
        reqwest::Client::new(),
        format!("http://{addr}"),
        "key".to_string(),
    );

    let record = sample_record().with_external_id("message-id-123");
    client.ingest(&record).await.unwrap();
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpIngestionClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1".to_string(),
        "key".to_string(),
    );
    let err = client.ingest(&sample_record()).await.unwrap_err();
    assert!(matches!(err, IngestionClientError::Unreachable(_)));
}
