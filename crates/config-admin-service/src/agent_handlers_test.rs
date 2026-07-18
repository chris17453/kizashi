use super::*;
use crate::agent_repository::agent_repository_test::{
    FailingAgentRepository, InMemoryAgentRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use tower::ServiceExt;

fn router(state: AgentState) -> Router {
    Router::new()
        .route("/v1/agents", post(create_agent).get(list_agents))
        .route("/v1/agents/:id", get(get_agent).put(update_agent).delete(delete_agent))
        .with_state(state)
}

fn sample_agent(tenant_id: Uuid) -> Agent {
    Agent::new(
        tenant_id,
        "zendesk",
        "support-poller",
        serde_json::json!({"url": "https://example.zendesk.com"}),
    )
}

fn default_state() -> AgentState {
    AgentState { agent_repository: Arc::new(InMemoryAgentRepository::default()) }
}

async fn send(
    app: Router,
    method: &str,
    uri: String,
    tenant_header: Option<Uuid>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(tenant_id) = tenant_header {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_agent_succeeds_when_tenant_matches() {
    let tenant_id = Uuid::new_v4();
    let agent = sample_agent(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/agents".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&agent).unwrap()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_agent_is_rejected_when_tenant_does_not_match() {
    let agent = sample_agent(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "POST",
        "/v1/agents".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&agent).unwrap()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_agents_is_scoped_to_the_header_tenant() {
    let tenant_id = Uuid::new_v4();
    let state = AgentState {
        agent_repository: Arc::new(InMemoryAgentRepository::with_agent(sample_agent(tenant_id))),
    };
    let response =
        send(router(state), "GET", "/v1/agents".to_string(), Some(tenant_id), None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let agents: Vec<Agent> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(agents.len(), 1);
}

#[tokio::test]
async fn get_agent_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/agents/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_agent_succeeds_then_get_returns_404() {
    let tenant_id = Uuid::new_v4();
    let agent = sample_agent(tenant_id);
    let state = AgentState {
        agent_repository: Arc::new(InMemoryAgentRepository::with_agent(agent.clone())),
    };
    let app = router(state);

    let delete_response =
        send(app.clone(), "DELETE", format!("/v1/agents/{}", agent.id), Some(tenant_id), None)
            .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_response =
        send(app, "GET", format!("/v1/agents/{}", agent.id), Some(tenant_id), None).await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn repository_failure_returns_500() {
    let tenant_id = Uuid::new_v4();
    let state = AgentState { agent_repository: Arc::new(FailingAgentRepository) };
    let response =
        send(router(state), "GET", "/v1/agents".to_string(), Some(tenant_id), None).await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
