use super::*;
use crate::agents_client::agents_client_test::InMemoryAgentsClient;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::{FailingBacklogClient, InMemoryBacklogClient};
use crate::backlog_client::QueueDepthSummary;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::{FailingHealthClient, InMemoryHealthClient};
use crate::health_client::{PlatformHealthSummary, ServiceHealthSummary};
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
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
    Router::new().route("/pipeline", get(get_pipeline)).with_state(state)
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
            summary: PlatformHealthSummary {
                status: "up".to_string(),
                services: vec![
                    ServiceHealthSummary {
                        name: "ingestion-service".to_string(),
                        status: "up".to_string(),
                    },
                    ServiceHealthSummary {
                        name: "normalization-service".to_string(),
                        status: "up".to_string(),
                    },
                    ServiceHealthSummary {
                        name: "analysis-service".to_string(),
                        status: "down".to_string(),
                    },
                ],
            },
        }),
        agents_client: Arc::new(InMemoryAgentsClient::default()),
        api_keys_client: Arc::new(InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(InMemoryBacklogClient::default()),
        execution_client: std::sync::Arc::new(
            crate::execution_client::execution_client_test::InMemoryExecutionClient::default(),
        ),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id)
}

#[tokio::test]
async fn renders_all_five_pipeline_stages_with_their_health_status() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Ingestion"));
    assert!(body.contains("Trigger Engine"));
    assert!(body.contains("Action Executor"));
    assert!(body.contains("node-up"));
    assert!(body.contains("node-down"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/pipeline").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_an_error_when_health_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.health_client = Arc::new(FailingHealthClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
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
async fn degrades_gracefully_showing_stages_with_no_backlog_numbers_when_backlog_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.backlog_client = Arc::new(FailingBacklogClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Ingestion"));
    assert!(body.contains("n/a"));
}

#[tokio::test]
async fn marks_a_heavily_backlogged_queue_as_critical() {
    let (state, session_id) = state_with_session().await;
    let backlog = InMemoryBacklogClient::default();
    backlog.depths.lock().unwrap().push(QueueDepthSummary {
        stage: "ingest_to_normalize".to_string(),
        queue_name: "normalization-service.record.ingested".to_string(),
        messages: 500,
    });
    let state = AppState { backlog_client: Arc::new(backlog), ..state };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/pipeline")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("edge-critical"));
    assert!(body.contains("500 queued"));
}
