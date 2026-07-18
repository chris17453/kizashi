use super::*;
use crate::agents_client::agents_client_test::InMemoryAgentsClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::{
    FailingIngestionStatsClient, InMemoryIngestionStatsClient,
};
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/data", get(get_data)).with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
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
    (state, session_id)
}

#[tokio::test]
async fn renders_search_results_when_signed_in() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: Uuid::new_v4(),
        connector_id: "zendesk".to_string(),
        source_type: "ticket".to_string(),
        ingested_at: chrono::Utc::now(),
        raw_payload: serde_json::json!({"subject": "printer on fire"}),
        normalized_payload: None,
    });
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?q=printer")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("zendesk"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/data").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.stats_client = Arc::new(FailingIngestionStatsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
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

#[tokio::test]
async fn renders_the_subject_and_email_from_search_fields_prefilled() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?subject=printer&email_from=alice%40example.com")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(r#"value="printer""#));
    assert!(body.contains("alice@example.com"));
}

#[tokio::test]
async fn shows_a_next_link_when_there_are_more_results_but_no_previous_link_on_page_zero() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    *stats_client.has_more.lock().unwrap() = true;
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data")
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
async fn shows_a_previous_link_on_page_two_and_no_next_link_when_no_more_results() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/data?page=1")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Previous"));
    assert!(!body.contains("Next"));
    assert!(body.contains("Page 2"));
}
