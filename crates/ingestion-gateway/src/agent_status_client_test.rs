use super::*;
use axum::extract::Path;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;

#[derive(Default)]
pub struct InMemoryAgentStatusClient {
    pub status: std::sync::Mutex<AgentStatus>,
}

#[async_trait]
impl AgentStatusClient for InMemoryAgentStatusClient {
    async fn status_for(
        &self,
        _tenant_id: Uuid,
        _connector_id: &str,
    ) -> Result<AgentStatus, AgentStatusError> {
        Ok(*self.status.lock().unwrap())
    }
}

pub struct FailingAgentStatusClient;

#[async_trait]
impl AgentStatusClient for FailingAgentStatusClient {
    async fn status_for(
        &self,
        _tenant_id: Uuid,
        _connector_id: &str,
    ) -> Result<AgentStatus, AgentStatusError> {
        Err(AgentStatusError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server(enabled_by_name: &'static str) -> String {
    async fn handler(Path(name): Path<String>) -> axum::response::Response {
        match name.as_str() {
            "enabled-agent" => {
                Json(serde_json::json!({"id": Uuid::new_v4(), "tenant_id": Uuid::new_v4(), "connector_type": "zendesk", "name": "enabled-agent", "config": {}, "enabled": true})).into_response()
            }
            "disabled-agent" => {
                Json(serde_json::json!({"id": Uuid::new_v4(), "tenant_id": Uuid::new_v4(), "connector_type": "zendesk", "name": "disabled-agent", "config": {}, "enabled": false})).into_response()
            }
            _ => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }
    let _ = enabled_by_name;
    let app = Router::new().route("/v1/agents/by-name/:name", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn returns_enabled_for_a_registered_and_enabled_agent() {
    let url = spawn_stub_server("enabled-agent").await;
    let client = HttpAgentStatusClient::new(reqwest::Client::new(), url);

    let status = client.status_for(Uuid::new_v4(), "enabled-agent").await.unwrap();
    assert_eq!(status, AgentStatus::Enabled);
}

#[tokio::test]
async fn returns_disabled_for_a_registered_and_disabled_agent() {
    let url = spawn_stub_server("disabled-agent").await;
    let client = HttpAgentStatusClient::new(reqwest::Client::new(), url);

    let status = client.status_for(Uuid::new_v4(), "disabled-agent").await.unwrap();
    assert_eq!(status, AgentStatus::Disabled);
}

#[tokio::test]
async fn returns_unregistered_for_a_404() {
    let url = spawn_stub_server("enabled-agent").await;
    let client = HttpAgentStatusClient::new(reqwest::Client::new(), url);

    let status = client.status_for(Uuid::new_v4(), "no-such-agent").await.unwrap();
    assert_eq!(status, AgentStatus::Unregistered);
}

#[tokio::test]
async fn returns_unreachable_when_server_is_down() {
    let client =
        HttpAgentStatusClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.status_for(Uuid::new_v4(), "anything").await.unwrap_err();
    assert!(matches!(err, AgentStatusError::Unreachable(_)));
}
