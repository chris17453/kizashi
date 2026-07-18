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

    let found = client.get_trigger(trigger.id).await.unwrap();
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

#[tokio::test]
async fn http_client_parses_a_found_trigger() {
    let trigger = sample_trigger();
    let url = spawn_stub_trigger_engine(Some(trigger.clone())).await;
    let client = HttpTriggerClient::new(reqwest::Client::new(), url);

    let found = client.get_trigger(trigger.id).await.unwrap();
    assert_eq!(found, Some(trigger));
}

#[tokio::test]
async fn http_client_returns_none_on_404() {
    let url = spawn_stub_trigger_engine(None).await;
    let client = HttpTriggerClient::new(reqwest::Client::new(), url);

    let found = client.get_trigger(Uuid::new_v4()).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpTriggerClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.get_trigger(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, TriggerClientError::Unreachable(_)));
}
