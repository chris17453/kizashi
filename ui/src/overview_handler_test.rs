use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::{PlatformHealthSummary, ServiceHealthSummary};
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::sensors_client::SensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::ConnectorStatSummary;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use common::Role;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/overview", get(get_overview)).with_state(state)
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
            created_at: chrono::Utc::now(),
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(crate::branding_client::branding_client_test::InMemoryBrandingClient::default()),
        oidc_client: Arc::new(crate::oidc_client::oidc_client_test::InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(crate::pending_oidc_flow::InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        sensors_client: Arc::new(InMemorySensorsClient::default()),
        api_keys_client: Arc::new(
            crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default(),
        ),
        backlog_client: Arc::new(
            crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default(),
        ),
        execution_client: std::sync::Arc::new(
            crate::execution_client::execution_client_test::InMemoryExecutionClient::default(),
        ),
        analysis_config_client: std::sync::Arc::new(crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient::default()),
        normalization_mappings_client: std::sync::Arc::new(crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: std::sync::Arc::new(crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: std::sync::Arc::new(crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        retention_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        auth_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn renders_kpi_cards_reflecting_real_data_when_signed_in() {
    let (mut state, session_id, tenant_id) = state_with_session().await;

    let sensors_client = Arc::new(InMemorySensorsClient::default());
    sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    sensors_client
        .register_sensor(
            Role::Operator,
            "test-actor",
            tenant_id,
            "sql",
            "never-run-sensor",
            serde_json::json!({}),
        )
        .await
        .unwrap();
    state.sensors_client = sensors_client;

    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.stats.lock().unwrap().push(ConnectorStatSummary {
        connector_id: "support-poller".to_string(),
        record_count: 42,
        last_ingested_at: chrono::Utc::now(),
    });
    state.stats_client = stats_client;

    state.health_client = Arc::new(InMemoryHealthClient {
        summary: PlatformHealthSummary {
            status: "up".to_string(),
            services: vec![
                ServiceHealthSummary { name: "a".to_string(), status: "up".to_string() },
                ServiceHealthSummary { name: "b".to_string(), status: "down".to_string() },
            ],
        },
    });

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(">2<")); // sensor_count
    assert!(body.contains("1 active")); // only support-poller has matching stats
    assert!(body.contains(">42<")); // total_records
    assert!(body.contains("1/2 services up"));
}

#[tokio::test]
async fn shows_the_five_most_recent_events_as_recent_activity() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;

    let events_client = Arc::new(InMemoryEventsClient::default());
    {
        let mut events = events_client.events.lock().unwrap();
        for i in 0..7 {
            events.push(crate::events_client::EventSummary {
                id: Uuid::new_v4(),
                event_type: format!("event-type-{i}"),
                group_key: format!("group-{i}"),
                status: "open".to_string(),
                occurred_at: chrono::Utc::now(),
                record_ids: vec![],
            });
        }
    }
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    // first 5 (0-4) shown, the rest (5, 6) not shown on the dashboard preview
    assert!(body.contains("event-type-0"));
    assert!(body.contains("event-type-4"));
    assert!(!body.contains("event-type-5"));
    assert!(!body.contains("event-type-6"));
}

#[tokio::test]
async fn shows_an_empty_state_for_recent_activity_when_there_are_no_events() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No recent activity"));
}

#[tokio::test]
async fn a_backend_failure_is_surfaced_not_silently_shown_as_zero() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    state.sensors_client =
        Arc::new(crate::sensors_client::sensors_client_test::FailingSensorsClient);
    state.health_client = Arc::new(crate::health_client::health_client_test::FailingHealthClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    // Previously every backend call silently `.unwrap_or_default()`'d, so a real outage
    // rendered as a plausible "0 sensors" dashboard indistinguishable from a healthy idle
    // tenant -- assert the failure is now visible on the page, not just that it doesn't crash.
    assert!(body.contains("class=\"error\""), "a backend failure should render visibly");
    assert!(body.contains("sensors:"));
    assert!(body.contains("platform health:"));
}

#[tokio::test]
async fn admin_only_nav_links_hidden_for_non_admin_and_shown_for_admin() {
    let (state, admin_session_id, tenant_id) = state_with_session().await;

    let viewer_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "bob".to_string(),
            role: common::Role::Viewer,
            created_at: chrono::Utc::now(),
        })
        .await;

    let viewer_response = router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={viewer_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(viewer_response.status(), StatusCode::OK);
    let viewer_bytes = axum::body::to_bytes(viewer_response.into_body(), usize::MAX).await.unwrap();
    let viewer_body = String::from_utf8(viewer_bytes.to_vec()).unwrap();
    assert!(
        !viewer_body.contains("href=\"/users\""),
        "a Viewer session should not see the admin-only Users nav link"
    );

    let admin_response = router(state)
        .oneshot(
            Request::builder()
                .uri("/overview")
                .header("cookie", format!("kizashi_session={admin_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::OK);
    let admin_bytes = axum::body::to_bytes(admin_response.into_body(), usize::MAX).await.unwrap();
    let admin_body = String::from_utf8(admin_bytes.to_vec()).unwrap();
    assert!(
        admin_body.contains("href=\"/users\""),
        "an Admin session should see the admin-only Users nav link"
    );
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/overview").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
