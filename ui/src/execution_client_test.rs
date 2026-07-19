use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryExecutionClient {
    pub executions: Mutex<Vec<ActionExecutionSummary>>,
}

#[async_trait]
impl ExecutionClient for InMemoryExecutionClient {
    async fn list_executions_for_event(
        &self,
        _tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecutionSummary>, ExecutionClientError> {
        Ok(self
            .executions
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.event_id == event_id)
            .cloned()
            .collect())
    }
}

pub struct FailingExecutionClient;

#[async_trait]
impl ExecutionClient for FailingExecutionClient {
    async fn list_executions_for_event(
        &self,
        _tenant_id: Uuid,
        _event_id: Uuid,
    ) -> Result<Vec<ActionExecutionSummary>, ExecutionClientError> {
        Err(ExecutionClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "trigger_id": "22222222-2222-2222-2222-222222222222",
            "event_id": "33333333-3333-3333-3333-333333333333",
            "action_type": "webhook",
            "status": "sent",
            "executed_at": "2026-07-19T00:00:00Z",
            "detail": {}
        }]))
        .into_response()
    }
    let app = Router::new().route("/v1/action-executions", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_executions_for_an_event_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpExecutionClient::new(reqwest::Client::new(), url);

    let executions =
        client.list_executions_for_event(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].status, "sent");
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpExecutionClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_executions_for_event(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, ExecutionClientError::Unreachable(_)));
}
