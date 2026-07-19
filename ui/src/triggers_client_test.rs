use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use common::TriggerDefinition;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTriggersClient {
    pub triggers: Mutex<Vec<TriggerSummary>>,
    pub has_more: Mutex<bool>,
    pub created: Mutex<Vec<TriggerDefinition>>,
    pub test_result: Mutex<Option<TriggerTestResult>>,
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

    async fn create_trigger(
        &self,
        role: Role,
        trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggersClientError> {
        if !role.at_least(Role::Operator) {
            return Err(TriggersClientError::Rejected(403));
        }
        self.created.lock().unwrap().push(trigger.clone());
        Ok(trigger)
    }

    async fn test_trigger(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _group_key: &str,
    ) -> Result<TriggerTestResult, TriggersClientError> {
        Ok(self
            .test_result
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(TriggerTestResult { would_fire: false, contributing_record_count: 0 }))
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

    async fn create_trigger(
        &self,
        _role: Role,
        _trigger: TriggerDefinition,
    ) -> Result<TriggerDefinition, TriggersClientError> {
        Err(TriggersClientError::Unreachable("simulated failure".to_string()))
    }

    async fn test_trigger(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _group_key: &str,
    ) -> Result<TriggerTestResult, TriggersClientError> {
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
    async fn create_handler(
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        (axum::http::StatusCode::CREATED, Json(body)).into_response()
    }
    let app = Router::new().route("/v1/trigger-definitions", get(handler).post(create_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn spawn_test_trigger_engine_stub() -> String {
    async fn test_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({"would_fire": true, "contributing_record_count": 3}))
            .into_response()
    }
    let app = Router::new().route("/v1/triggers/:id/test", post(test_handler));
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
    let client = HttpTriggersClient::new(reqwest::Client::new(), url, String::new());

    let page = client.list_triggers(Uuid::new_v4(), 25, 0).await.unwrap();

    assert_eq!(page.triggers.len(), 1);
    assert_eq!(page.triggers[0].name, "high-volume-negative");
    assert!(page.triggers[0].enabled);
    assert!(!page.has_more);
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpTriggersClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1".to_string(),
        String::new(),
    );
    let err = client.list_triggers(Uuid::new_v4(), 25, 0).await.unwrap_err();
    assert!(matches!(err, TriggersClientError::Unreachable(_)));
}

fn sample_trigger() -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "urgent-spike".to_string(),
        event_type_match: "priority_score".to_string(),
        condition: common::TriggerCondition::ThresholdOverWindow {
            field: "priority_score".to_string(),
            threshold: 5.0,
            direction: common::ThresholdDirection::Above,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn http_client_creates_a_trigger_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpTriggersClient::new(reqwest::Client::new(), url, String::new());

    let created = client.create_trigger(Role::Operator, sample_trigger()).await.unwrap();
    assert_eq!(created.name, "urgent-spike");
}

#[tokio::test]
async fn http_client_create_is_rejected_for_insufficient_role() {
    let url = spawn_stub_server().await;
    let client = HttpTriggersClient::new(reqwest::Client::new(), url, String::new());

    let err = client.create_trigger(Role::Viewer, sample_trigger()).await.unwrap_err();
    assert!(matches!(err, TriggersClientError::Rejected(403)));
}

#[tokio::test]
async fn http_client_tests_a_trigger_against_a_real_trigger_engine_server() {
    let trigger_engine_url = spawn_test_trigger_engine_stub().await;
    let client = HttpTriggersClient::new(reqwest::Client::new(), String::new(), trigger_engine_url);

    let result = client.test_trigger(Uuid::new_v4(), Uuid::new_v4(), "cust-1").await.unwrap();

    assert!(result.would_fire);
    assert_eq!(result.contributing_record_count, 3);
}

#[tokio::test]
async fn http_client_test_trigger_returns_unreachable_when_trigger_engine_is_down() {
    let client = HttpTriggersClient::new(
        reqwest::Client::new(),
        String::new(),
        "http://127.0.0.1:1".to_string(),
    );
    let err = client.test_trigger(Uuid::new_v4(), Uuid::new_v4(), "cust-1").await.unwrap_err();
    assert!(matches!(err, TriggersClientError::Unreachable(_)));
}
