use super::*;
use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use common::TriggerCondition;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryTriggerClient {
    pub triggers: Mutex<Vec<TriggerDefinition>>,
}

impl InMemoryTriggerClient {
    pub fn with_trigger(trigger: TriggerDefinition) -> Self {
        Self { triggers: Mutex::new(vec![trigger]) }
    }
}

#[async_trait]
impl TriggerClient for InMemoryTriggerClient {
    async fn get_trigger(
        &self,
        trigger_id: Uuid,
        _tenant_id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerClientError> {
        Ok(self.triggers.lock().unwrap().iter().find(|t| t.id == trigger_id).cloned())
    }
}

pub struct FailingTriggerClient;

#[async_trait]
impl TriggerClient for FailingTriggerClient {
    async fn get_trigger(
        &self,
        _trigger_id: Uuid,
        _tenant_id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerClientError> {
        Err(TriggerClientError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_trigger() -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "test".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 3 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn in_memory_client_finds_a_known_trigger() {
    let trigger = sample_trigger();
    let client = InMemoryTriggerClient::with_trigger(trigger.clone());

    let found = client.get_trigger(trigger.id, trigger.tenant_id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

async fn spawn_stub_trigger_engine(trigger: Option<TriggerDefinition>) -> String {
    async fn handler(
        axum::extract::State(trigger): axum::extract::State<Option<TriggerDefinition>>,
        Path(_id): Path<Uuid>,
    ) -> axum::response::Response {
        match trigger {
            Some(t) => axum::Json(t).into_response(),
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }
    let app = Router::new().route("/v1/triggers/:id", get(handler)).with_state(trigger);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Only accepts the request if `X-Tenant-Id` matches the one baked in at spawn time; otherwise
/// 401 -- used to prove the client actually sends the header with the right value, not just
/// that a request with the header present happens to succeed.
async fn spawn_stub_trigger_engine_requiring_tenant(
    trigger: TriggerDefinition,
    expected_tenant_id: Uuid,
) -> String {
    #[derive(Clone)]
    struct Fixture {
        trigger: TriggerDefinition,
        expected_tenant_id: Uuid,
    }
    async fn handler(
        axum::extract::State(fixture): axum::extract::State<Fixture>,
        headers: axum::http::HeaderMap,
        Path(_id): Path<Uuid>,
    ) -> axum::response::Response {
        let provided = headers.get("x-tenant-id").and_then(|v| v.to_str().ok());
        if provided != Some(&fixture.expected_tenant_id.to_string()) {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        axum::Json(fixture.trigger).into_response()
    }
    let app = Router::new()
        .route("/v1/triggers/:id", get(handler))
        .with_state(Fixture { trigger, expected_tenant_id });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_parses_a_found_trigger() {
    let trigger = sample_trigger();
    let url = spawn_stub_trigger_engine(Some(trigger.clone())).await;
    let client = HttpTriggerClient::new(reqwest::Client::new(), url);

    let found = client.get_trigger(trigger.id, trigger.tenant_id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn http_client_sends_the_tenant_id_header() {
    let trigger = sample_trigger();
    let url = spawn_stub_trigger_engine_requiring_tenant(trigger.clone(), trigger.tenant_id).await;
    let client = HttpTriggerClient::new(reqwest::Client::new(), url);

    let found = client.get_trigger(trigger.id, trigger.tenant_id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn http_client_is_rejected_when_it_sends_the_wrong_tenant_id() {
    let trigger = sample_trigger();
    let url = spawn_stub_trigger_engine_requiring_tenant(trigger.clone(), trigger.tenant_id).await;
    let client = HttpTriggerClient::new(reqwest::Client::new(), url);

    let result = client.get_trigger(trigger.id, Uuid::new_v4()).await;
    assert!(matches!(result, Err(TriggerClientError::Rejected(401))));
}

#[tokio::test]
async fn http_client_returns_none_on_404() {
    let url = spawn_stub_trigger_engine(None).await;
    let client = HttpTriggerClient::new(reqwest::Client::new(), url);

    let found = client.get_trigger(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpTriggerClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.get_trigger(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, TriggerClientError::Unreachable(_)));
}
