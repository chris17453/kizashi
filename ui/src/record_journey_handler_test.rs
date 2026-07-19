use super::*;
use crate::agents_client::agents_client_test::InMemoryAgentsClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::{
    FailingExecutionClient, InMemoryExecutionClient,
};
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
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/data/:id/journey", get(get_record_journey)).with_state(state)
}

async fn state_with_session() -> (AppState, String, Uuid) {
    let session_store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
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
        agents_client: Arc::new(InMemoryAgentsClient::default()),
        api_keys_client: Arc::new(
            crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default(),
        ),
        backlog_client: Arc::new(
            crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default(),
        ),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn renders_record_with_no_events_yet() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let record_id = Uuid::new_v4();
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: record_id,
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
                .uri(format!("/data/{record_id}/journey"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("hasn't contributed to any events yet"));
}

#[tokio::test]
async fn renders_events_and_their_executions() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let record_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();

    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: record_id,
        connector_id: "zendesk".to_string(),
        source_type: "ticket".to_string(),
        ingested_at: chrono::Utc::now(),
        raw_payload: serde_json::json!({}),
        normalized_payload: None,
    });
    state.stats_client = stats_client;

    let events_client = Arc::new(InMemoryEventsClient::default());
    events_client.events.lock().unwrap().push(EventSummary {
        id: event_id,
        event_type: "spike".to_string(),
        group_key: "zendesk:ticket".to_string(),
        status: "open".to_string(),
        occurred_at: chrono::Utc::now(),
    });
    state.events_client = events_client;

    let execution_client = Arc::new(InMemoryExecutionClient::default());
    execution_client.executions.lock().unwrap().push(ActionExecutionSummary {
        id: Uuid::new_v4(),
        trigger_id: Uuid::new_v4(),
        event_id,
        action_type: "webhook".to_string(),
        status: "sent".to_string(),
        executed_at: chrono::Utc::now(),
        detail: serde_json::json!({}),
    });
    state.execution_client = execution_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/data/{record_id}/journey"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("spike"));
    assert!(body.contains("webhook"));
    assert!(body.contains("sent"));
}

#[tokio::test]
async fn renders_event_with_no_executions_when_execution_client_fails() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let record_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();

    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.records.lock().unwrap().push(RecordSummary {
        id: record_id,
        connector_id: "zendesk".to_string(),
        source_type: "ticket".to_string(),
        ingested_at: chrono::Utc::now(),
        raw_payload: serde_json::json!({}),
        normalized_payload: None,
    });
    state.stats_client = stats_client;

    let events_client = Arc::new(InMemoryEventsClient::default());
    events_client.events.lock().unwrap().push(EventSummary {
        id: event_id,
        event_type: "spike".to_string(),
        group_key: "zendesk:ticket".to_string(),
        status: "open".to_string(),
        occurred_at: chrono::Utc::now(),
    });
    state.events_client = events_client;
    state.execution_client = Arc::new(FailingExecutionClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/data/{record_id}/journey"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("spike"));
    assert!(body.contains("No actions executed yet"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/data/{}/journey", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
