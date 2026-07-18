use super::*;
use axum::routing::post;
use axum::{Json, Router};
use common::SourceType;
use serde_json::json;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAnalysisClient {
    pub calls: Mutex<Vec<(Uuid, usize)>>,
}

#[async_trait]
impl AnalysisClient for InMemoryAnalysisClient {
    async fn analyze_batch(
        &self,
        tenant_id: Uuid,
        records: &[RawRecord],
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        self.calls.lock().unwrap().push((tenant_id, records.len()));
        Ok(records.iter().map(|_| json!({"sentiment": -0.5})).collect())
    }
}

pub struct FailingAnalysisClient;

#[async_trait]
impl AnalysisClient for FailingAnalysisClient {
    async fn analyze_batch(
        &self,
        _tenant_id: Uuid,
        _records: &[RawRecord],
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        Err(AnalysisError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_record() -> RawRecord {
    RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({"description": "hi"}))
}

#[tokio::test]
async fn in_memory_client_returns_one_result_per_record() {
    let client = InMemoryAnalysisClient::default();
    let records = vec![sample_record(), sample_record()];

    let results = client.analyze_batch(Uuid::new_v4(), &records).await.unwrap();
    assert_eq!(results.len(), 2);
}

async fn spawn_stub_foundry(
    results: Vec<serde_json::Value>,
    status: axum::http::StatusCode,
) -> String {
    async fn handler(
        axum::extract::State((results, status)): axum::extract::State<(
            Vec<serde_json::Value>,
            axum::http::StatusCode,
        )>,
        Json(_body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        (status, Json(json!({"results": results}))).into_response()
    }
    let app = Router::new().route("/analyze", post(handler)).with_state((results, status));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/analyze")
}

#[tokio::test]
async fn foundry_client_parses_a_successful_response() {
    let endpoint =
        spawn_stub_foundry(vec![json!({"sentiment": -0.5})], axum::http::StatusCode::OK).await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    let results = client.analyze_batch(Uuid::new_v4(), &[sample_record()]).await.unwrap();
    assert_eq!(results, vec![json!({"sentiment": -0.5})]);
}

#[tokio::test]
async fn foundry_client_returns_rejected_on_non_success_status() {
    let endpoint = spawn_stub_foundry(vec![], axum::http::StatusCode::INTERNAL_SERVER_ERROR).await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()]).await.unwrap_err();
    assert!(matches!(err, AnalysisError::Rejected(500)));
}

#[tokio::test]
async fn foundry_client_returns_mismatch_when_result_count_differs() {
    let endpoint = spawn_stub_foundry(vec![json!({}), json!({})], axum::http::StatusCode::OK).await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()]).await.unwrap_err();
    assert!(matches!(err, AnalysisError::ResultCountMismatch { expected: 1, got: 2 }));
}

#[tokio::test]
async fn foundry_client_returns_unreachable_when_server_is_down() {
    let client = FoundryAnalysisClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1/analyze".to_string(),
        "test-key".to_string(),
    );
    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()]).await.unwrap_err();
    assert!(matches!(err, AnalysisError::Unreachable(_)));
}
