use super::*;
use crate::agents_client::agents_client_test::{FailingAgentsClient, InMemoryAgentsClient};
use crate::agents_client::AgentsClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/agents", get(get_agents).post(post_agents))
        .route("/agents/:id/delete", post(post_delete_agent))
        .route("/agents/:id/toggle", post(post_toggle_agent))
        .with_state(state)
}

async fn state_with_session() -> (AppState, String, Uuid) {
    let session_store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "alice".to_string(),
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        agents_client: Arc::new(InMemoryAgentsClient::default()),
        api_keys_client: Arc::new(
            crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default(),
        ),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn get_agents_renders_the_agents_table_when_signed_in() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .agents_client
        .register_agent(tenant_id, "zendesk", "support-poller", serde_json::json!({}))
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("support-poller"));
}

#[tokio::test]
async fn get_agents_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/agents").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn post_agents_registers_and_redirects() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let agents_client = Arc::new(InMemoryAgentsClient::default());
    state.agents_client = agents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("connector_type=zendesk&name=support-poller&config={}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/agents");
    assert_eq!(agents_client.agents.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn post_agents_with_invalid_json_config_rerenders_with_an_error() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("connector_type=zendesk&name=support-poller&config=not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("must be valid JSON"));
}

#[tokio::test]
async fn post_agents_backend_failure_rerenders_with_an_error() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    state.agents_client = Arc::new(FailingAgentsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("connector_type=zendesk&name=support-poller&config={}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("unreachable"));
}

#[tokio::test]
async fn post_delete_agent_removes_it_and_redirects() {
    let (mut state, session_id, tenant_id) = state_with_session().await;
    let agents_client = Arc::new(InMemoryAgentsClient::default());
    let agent = agents_client
        .register_agent(tenant_id, "zendesk", "support-poller", serde_json::json!({}))
        .await
        .unwrap();
    state.agents_client = agents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/agents/{}/delete", agent.id))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(agents_client.agents.lock().unwrap().is_empty());
}

#[tokio::test]
async fn post_toggle_agent_flips_enabled_and_redirects() {
    let (mut state, session_id, tenant_id) = state_with_session().await;
    let agents_client = Arc::new(InMemoryAgentsClient::default());
    let agent = agents_client
        .register_agent(tenant_id, "zendesk", "support-poller", serde_json::json!({}))
        .await
        .unwrap();
    assert!(agent.enabled);
    state.agents_client = agents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/agents/{}/toggle", agent.id))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let stored = agents_client.agents.lock().unwrap();
    assert!(!stored.iter().find(|a| a.id == agent.id).unwrap().enabled);
}

#[tokio::test]
async fn post_toggle_agent_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/agents/{}/toggle", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn shows_a_next_link_when_there_are_more_agents_but_no_previous_link_on_page_zero() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let agents_client = Arc::new(InMemoryAgentsClient::default());
    *agents_client.has_more.lock().unwrap() = true;
    state.agents_client = agents_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Next"));
    assert!(!body.contains("Previous"));
}

#[tokio::test]
async fn shows_a_previous_link_on_page_two() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/agents?page=1")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Previous"));
    assert!(body.contains("Page 2"));
}
