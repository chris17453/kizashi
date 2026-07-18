use super::*;
use crate::agents_client::agents_client_test::InMemoryAgentsClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new().route("/agents/:id", get(get_agent_detail)).with_state(state)
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
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
    };
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn renders_the_agent_and_its_records_when_found() {
    let (mut state, session_id, tenant_id) = state_with_session().await;
    let agent = state
        .agents_client
        .register_agent(tenant_id, "zendesk", "support-poller", serde_json::json!({}))
        .await
        .unwrap();
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: Uuid::new_v4(),
        source_type: "ticket".to_string(),
        ingested_at: chrono::Utc::now(),
        normalized_payload: None,
    });
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/agents/{}", agent.id))
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
    assert!(body.contains("ticket"));
}

#[tokio::test]
async fn renders_not_found_for_an_unknown_agent_id() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/agents/{}", Uuid::new_v4()))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Agent not found"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/agents/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
