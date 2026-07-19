use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::session::SessionStore;
use crate::session::{InMemorySessionStore, Session};
use crate::triggers_client::triggers_client_test::{FailingTriggersClient, InMemoryTriggersClient};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/triggers", get(get_triggers)).with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
            username: "alice".to_string(),
            role: common::Role::Admin,
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
        agents_client: Arc::new(crate::agents_client::agents_client_test::InMemoryAgentsClient::default()),
        api_keys_client: Arc::new(crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default()),
        execution_client: std::sync::Arc::new(crate::execution_client::execution_client_test::InMemoryExecutionClient::default()),
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id)
}

#[tokio::test]
async fn shows_an_empty_state_with_no_triggers_configured() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No triggers configured yet"));
    assert!(!body.contains("<table>"));
}

#[tokio::test]
async fn renders_the_triggers_table_when_signed_in() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    triggers_client.triggers.lock().unwrap().push(TriggerSummary {
        id: Uuid::new_v4(),
        name: "high-volume-negative".to_string(),
        event_type_match: "sentiment".to_string(),
        enabled: true,
    });
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("high-volume-negative"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/triggers").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_a_next_link_when_there_are_more_triggers_but_no_previous_link_on_page_zero() {
    let (mut state, session_id) = state_with_session().await;
    let triggers_client = InMemoryTriggersClient::default();
    *triggers_client.has_more.lock().unwrap() = true;
    state.triggers_client = Arc::new(triggers_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
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
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers?page=1")
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

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.triggers_client = Arc::new(FailingTriggersClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/triggers")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("unreachable"));
}
