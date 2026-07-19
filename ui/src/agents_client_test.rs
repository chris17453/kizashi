use super::*;
use axum::extract::Json as JsonExtractor;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAgentsClient {
    pub agents: Mutex<Vec<Agent>>,
    pub has_more: Mutex<bool>,
}

#[async_trait]
impl AgentsClient for InMemoryAgentsClient {
    async fn list_agents(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<AgentsPage, AgentsClientError> {
        Ok(AgentsPage {
            agents: self.agents.lock().unwrap().clone(),
            has_more: *self.has_more.lock().unwrap(),
        })
    }

    async fn get_agent(
        &self,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Agent>, AgentsClientError> {
        Ok(self.agents.lock().unwrap().iter().find(|a| a.id == id).cloned())
    }

    async fn register_agent(
        &self,
        tenant_id: Uuid,
        connector_type: &str,
        name: &str,
        config: serde_json::Value,
    ) -> Result<Agent, AgentsClientError> {
        let agent = Agent::new(tenant_id, connector_type, name, config);
        self.agents.lock().unwrap().push(agent.clone());
        Ok(agent)
    }

    async fn delete_agent(&self, _tenant_id: Uuid, id: Uuid) -> Result<(), AgentsClientError> {
        self.agents.lock().unwrap().retain(|a| a.id != id);
        Ok(())
    }

    async fn update_agent(&self, agent: &Agent) -> Result<Agent, AgentsClientError> {
        let mut agents = self.agents.lock().unwrap();
        match agents.iter_mut().find(|a| a.id == agent.id) {
            Some(existing) => {
                *existing = agent.clone();
                Ok(agent.clone())
            }
            None => Err(AgentsClientError::Rejected(404)),
        }
    }
}

pub struct FailingAgentsClient;

#[async_trait]
impl AgentsClient for FailingAgentsClient {
    async fn list_agents(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<AgentsPage, AgentsClientError> {
        Err(AgentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn get_agent(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Agent>, AgentsClientError> {
        Err(AgentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn register_agent(
        &self,
        _tenant_id: Uuid,
        _connector_type: &str,
        _name: &str,
        _config: serde_json::Value,
    ) -> Result<Agent, AgentsClientError> {
        Err(AgentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete_agent(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), AgentsClientError> {
        Err(AgentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn update_agent(&self, _agent: &Agent) -> Result<Agent, AgentsClientError> {
        Err(AgentsClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn list_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({
            "agents": [{
                "id": "11111111-1111-1111-1111-111111111111",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "connector_type": "zendesk",
                "name": "support-poller",
                "config": {},
                "enabled": true
            }],
            "has_more": false
        }))
        .into_response()
    }
    async fn get_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "connector_type": "zendesk",
            "name": "support-poller",
            "config": {},
            "enabled": true
        }))
        .into_response()
    }
    async fn create_handler(
        JsonExtractor(agent): JsonExtractor<Agent>,
    ) -> axum::response::Response {
        (axum::http::StatusCode::CREATED, Json(agent)).into_response()
    }
    async fn delete_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::NO_CONTENT
    }
    async fn update_handler(
        JsonExtractor(agent): JsonExtractor<Agent>,
    ) -> axum::response::Response {
        Json(agent).into_response()
    }
    let app = Router::new()
        .route("/v1/agents", get(list_handler).post(create_handler))
        .route("/v1/agents/:id", get(get_handler).delete(delete_handler).put(update_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_agents_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAgentsClient::new(reqwest::Client::new(), url);

    let page = client.list_agents(Uuid::new_v4(), 25, 0).await.unwrap();

    assert_eq!(page.agents.len(), 1);
    assert_eq!(page.agents[0].name, "support-poller");
    assert!(!page.has_more);
}

#[tokio::test]
async fn http_client_gets_an_agent_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAgentsClient::new(reqwest::Client::new(), url);

    let agent = client.get_agent(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    assert!(agent.is_some());
    assert_eq!(agent.unwrap().name, "support-poller");
}

#[tokio::test]
async fn http_client_registers_an_agent_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAgentsClient::new(reqwest::Client::new(), url);
    let tenant_id = Uuid::new_v4();

    let agent = client
        .register_agent(tenant_id, "zendesk", "support-poller", serde_json::json!({}))
        .await
        .unwrap();

    assert_eq!(agent.tenant_id, tenant_id);
    assert_eq!(agent.name, "support-poller");
}

#[tokio::test]
async fn http_client_deletes_an_agent_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAgentsClient::new(reqwest::Client::new(), url);

    client.delete_agent(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn http_client_updates_an_agent_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpAgentsClient::new(reqwest::Client::new(), url);
    let mut agent = Agent::new(Uuid::new_v4(), "zendesk", "support-poller", serde_json::json!({}));
    agent.enabled = false;

    let updated = client.update_agent(&agent).await.unwrap();
    assert!(!updated.enabled);
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpAgentsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_agents(Uuid::new_v4(), 25, 0).await.unwrap_err();
    assert!(matches!(err, AgentsClientError::Unreachable(_)));
}
