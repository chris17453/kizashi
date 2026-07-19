use super::*;
use axum::routing::post;
use axum::{Json, Router};
use common::SourceType;
use serde_json::json;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAnalysisClient {
    pub calls: Mutex<Vec<(Uuid, usize, Option<String>)>>,
}

#[async_trait]
impl AnalysisClient for InMemoryAnalysisClient {
    async fn analyze_batch(
        &self,
        tenant_id: Uuid,
        records: &[RawRecord],
        prompt: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        self.calls.lock().unwrap().push((tenant_id, records.len(), prompt.map(str::to_string)));
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
        _prompt: Option<&str>,
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

    let results = client.analyze_batch(Uuid::new_v4(), &records, None).await.unwrap();
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

    let results = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap();
    assert_eq!(results, vec![json!({"sentiment": -0.5})]);
}

#[tokio::test]
async fn foundry_client_returns_rejected_on_non_success_status() {
    let endpoint = spawn_stub_foundry(vec![], axum::http::StatusCode::INTERNAL_SERVER_ERROR).await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap_err();
    assert!(matches!(err, AnalysisError::Rejected(500)));
}

#[tokio::test]
async fn foundry_client_returns_mismatch_when_result_count_differs() {
    let endpoint = spawn_stub_foundry(vec![json!({}), json!({})], axum::http::StatusCode::OK).await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap_err();
    assert!(matches!(err, AnalysisError::ResultCountMismatch { expected: 1, got: 2 }));
}

async fn spawn_prompt_capturing_stub() -> (String, std::sync::Arc<Mutex<Option<String>>>) {
    let captured = std::sync::Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    async fn handler(
        axum::extract::State(captured): axum::extract::State<std::sync::Arc<Mutex<Option<String>>>>,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        *captured.lock().unwrap() = body.get("prompt").and_then(|v| v.as_str()).map(str::to_string);
        Json(json!({"results": [json!({"sentiment": -0.5})]})).into_response()
    }
    let app = Router::new().route("/analyze", post(handler)).with_state(captured_clone);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/analyze"), captured)
}

#[tokio::test]
async fn foundry_client_includes_the_prompt_in_the_request_body_when_present() {
    let (endpoint, captured) = spawn_prompt_capturing_stub().await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    client
        .analyze_batch(Uuid::new_v4(), &[sample_record()], Some("look for urgent tickets"))
        .await
        .unwrap();

    assert_eq!(*captured.lock().unwrap(), Some("look for urgent tickets".to_string()));
}

#[tokio::test]
async fn foundry_client_omits_the_prompt_field_when_none() {
    let (endpoint, captured) = spawn_prompt_capturing_stub().await;
    let client =
        FoundryAnalysisClient::new(reqwest::Client::new(), endpoint, "test-key".to_string());

    client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap();

    assert_eq!(*captured.lock().unwrap(), None);
}

#[tokio::test]
async fn foundry_client_returns_unreachable_when_server_is_down() {
    let client = FoundryAnalysisClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1/analyze".to_string(),
        "test-key".to_string(),
    );
    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap_err();
    assert!(matches!(err, AnalysisError::Unreachable(_)));
}

pub(crate) async fn spawn_stub_chat_completions(
    reply_content: String,
) -> (String, std::sync::Arc<Mutex<Vec<serde_json::Value>>>) {
    let captured = std::sync::Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    async fn handler(
        axum::extract::State((captured, reply_content)): axum::extract::State<(
            std::sync::Arc<Mutex<Vec<serde_json::Value>>>,
            String,
        )>,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        captured.lock().unwrap().push(body);
        Json(json!({
            "choices": [{"message": {"content": reply_content}}]
        }))
        .into_response()
    }
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .with_state((captured_clone, reply_content));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/v1"), captured)
}

#[tokio::test]
async fn openai_compatible_client_returns_one_parsed_json_result_per_record() {
    let (endpoint, _captured) =
        spawn_stub_chat_completions(r#"{"sentiment": -0.5}"#.to_string()).await;
    let client = OpenAiCompatibleAnalysisClient::new(
        reqwest::Client::new(),
        endpoint,
        None,
        "qwen3:8b".to_string(),
    );

    let records = vec![sample_record(), sample_record()];
    let results = client.analyze_batch(Uuid::new_v4(), &records, None).await.unwrap();

    assert_eq!(results, vec![json!({"sentiment": -0.5}), json!({"sentiment": -0.5})]);
}

#[tokio::test]
async fn openai_compatible_client_wraps_non_json_replies_as_text() {
    let (endpoint, _captured) = spawn_stub_chat_completions("looks urgent to me".to_string()).await;
    let client = OpenAiCompatibleAnalysisClient::new(
        reqwest::Client::new(),
        endpoint,
        None,
        "qwen3:8b".to_string(),
    );

    let results = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap();

    assert_eq!(results, vec![json!({"text": "looks urgent to me"})]);
}

#[tokio::test]
async fn openai_compatible_client_sends_the_model_and_a_bearer_key_when_present() {
    let (endpoint, captured) = spawn_stub_chat_completions(r#"{"ok": true}"#.to_string()).await;
    let client = OpenAiCompatibleAnalysisClient::new(
        reqwest::Client::new(),
        endpoint,
        Some("sk-test".to_string()),
        "gpt-4o-mini".to_string(),
    );

    client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap();

    let sent = captured.lock().unwrap();
    assert_eq!(sent[0]["model"], "gpt-4o-mini");
}

#[tokio::test]
async fn openai_compatible_client_includes_the_prompt_in_the_message_content_when_present() {
    let (endpoint, captured) = spawn_stub_chat_completions(r#"{"ok": true}"#.to_string()).await;
    let client = OpenAiCompatibleAnalysisClient::new(
        reqwest::Client::new(),
        endpoint,
        None,
        "qwen3:8b".to_string(),
    );

    client
        .analyze_batch(Uuid::new_v4(), &[sample_record()], Some("look for urgent tickets"))
        .await
        .unwrap();

    let sent = captured.lock().unwrap();
    let content = sent[0]["messages"][0]["content"].as_str().unwrap().to_string();
    assert!(content.contains("look for urgent tickets"));
}

#[tokio::test]
async fn openai_compatible_client_returns_unreachable_when_server_is_down() {
    let client = OpenAiCompatibleAnalysisClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1".to_string(),
        None,
        "qwen3:8b".to_string(),
    );
    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap_err();
    assert!(matches!(err, AnalysisError::Unreachable(_)));
}

#[tokio::test]
async fn openai_compatible_client_returns_rejected_on_non_success_status() {
    async fn handler() -> axum::http::StatusCode {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    }
    let app = Router::new().route("/v1/chat/completions", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = OpenAiCompatibleAnalysisClient::new(
        reqwest::Client::new(),
        format!("http://{addr}/v1"),
        None,
        "qwen3:8b".to_string(),
    );

    let err = client.analyze_batch(Uuid::new_v4(), &[sample_record()], None).await.unwrap_err();
    assert!(matches!(err, AnalysisError::Rejected(500)));
}
