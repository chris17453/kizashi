use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTriggersClient {
    pub triggers: Mutex<Vec<TriggerSummary>>,
    pub has_more: Mutex<bool>,
}

#[async_trait]
impl TriggersClient for InMemoryTriggersClient {
    async fn list_triggers(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<TriggersPage, TriggersClientError> {
        Ok(TriggersPage {
            triggers: self.triggers.lock().unwrap().clone(),
            has_more: *self.has_more.lock().unwrap(),
        })
    }
}

pub struct FailingTriggersClient;

#[async_trait]
impl TriggersClient for FailingTriggersClient {
    async fn list_triggers(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<TriggersPage, TriggersClientError> {
        Err(TriggersClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({
            "triggers": [{
                "id": "11111111-1111-1111-1111-111111111111",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "name": "high-volume-negative",
                "event_type_match": "sentiment",
                "condition": {"shape": "count_over_window", "count": 3},
                "window_seconds": 3600,
                "actions": [],
                "enabled": true
            }],
            "has_more": false
        }))
        .into_response()
    }
    let app = Router::new().route("/v1/trigger-definitions", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_triggers_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpTriggersClient::new(reqwest::Client::new(), url);

    let page = client.list_triggers(Uuid::new_v4(), 25, 0).await.unwrap();

    assert_eq!(page.triggers.len(), 1);
    assert_eq!(page.triggers[0].name, "high-volume-negative");
    assert!(page.triggers[0].enabled);
    assert!(!page.has_more);
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpTriggersClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_triggers(Uuid::new_v4(), 25, 0).await.unwrap_err();
    assert!(matches!(err, TriggersClientError::Unreachable(_)));
}
