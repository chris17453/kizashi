use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::{FailingEventsClient, InMemoryEventsClient};
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::session::SessionStore;
use crate::session::{InMemorySessionStore, Session};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/events", get(get_events)).with_state(state)
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
        analysis_config_client: std::sync::Arc::new(crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient::default()),
        normalization_mappings_client: std::sync::Arc::new(crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: std::sync::Arc::new(crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: std::sync::Arc::new(crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        retention_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        auth_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id)
}

#[tokio::test]
async fn shows_an_empty_state_with_no_events_recorded() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No events recorded yet"));
    assert!(!body.contains("<table>"));
}

#[tokio::test]
async fn renders_the_events_table_when_signed_in() {
    let (mut state, session_id) = state_with_session().await;
    let events_client = InMemoryEventsClient::default();
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "sentiment_spike".to_string(),
        group_key: "customer-42".to_string(),
        status: "open".to_string(),
        occurred_at: "2026-07-18T00:00:00Z".parse().unwrap(),
    });
    state.events_client = Arc::new(events_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("sentiment_spike"));
    assert!(body.contains("customer-42"));
}

#[tokio::test]
async fn renders_a_bar_for_each_day_with_events() {
    let (mut state, session_id) = state_with_session().await;
    let events_client = InMemoryEventsClient::default();
    events_client
        .daily_counts
        .lock()
        .unwrap()
        .push(crate::events_client::DailyCount { date: "2026-07-18".to_string(), count: 5 });
    state.events_client = Arc::new(events_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("2026-07-18"));
    assert!(body.contains("<svg"));
}

#[tokio::test]
async fn a_daily_counts_failure_does_not_break_the_rest_of_the_page() {
    let (mut state, session_id) = state_with_session().await;
    // events_client already succeeds for list_events (InMemoryEventsClient default), but we
    // want daily_counts specifically to fail — swap in a client whose daily_counts errors
    // while list_events still succeeds, proving the two are independent.
    struct EventsOkDailyCountsFailing;
    #[async_trait::async_trait]
    impl crate::events_client::EventsClient for EventsOkDailyCountsFailing {
        async fn list_events(
            &self,
            _bearer_token: &str,
            _limit: u32,
            _offset: u32,
        ) -> Result<crate::events_client::EventsPage, crate::events_client::EventsClientError>
        {
            Ok(crate::events_client::EventsPage { events: vec![], has_more: false })
        }
        async fn list_events_for_record(
            &self,
            _bearer_token: &str,
            _record_id: Uuid,
        ) -> Result<Vec<EventSummary>, crate::events_client::EventsClientError> {
            Ok(vec![])
        }
        async fn daily_counts(
            &self,
            _bearer_token: &str,
            _since: chrono::DateTime<chrono::Utc>,
            _until: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<crate::events_client::DailyCount>, crate::events_client::EventsClientError>
        {
            Err(crate::events_client::EventsClientError::Unreachable("simulated".to_string()))
        }
    }
    state.events_client = Arc::new(EventsOkDailyCountsFailing);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.events_client = Arc::new(FailingEventsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events")
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
async fn shows_a_next_link_when_there_are_more_results_but_no_previous_link_on_page_zero() {
    let (mut state, session_id) = state_with_session().await;
    let events_client = Arc::new(InMemoryEventsClient::default());
    *events_client.has_more.lock().unwrap() = true;
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events")
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
                .uri("/events?page=1")
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
