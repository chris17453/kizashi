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
use common::{Incident, IncidentSeverity, IncidentStatus, Role};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn signal_heatmap_scales_cells_to_the_busiest_day() {
    let cells = build_signal_heatmap(&[
        crate::events_client::DailyCount { date: "2026-07-21".into(), count: 2 },
        crate::events_client::DailyCount { date: "2026-07-22".into(), count: 8 },
    ]);
    assert_eq!(cells.iter().filter(|cell| !cell.blank).count(), 2);
    assert_eq!(cells[1].intensity, 1);
    assert_eq!(cells[2].intensity, 4);
    assert_eq!(cells[2].count, 8);
}

#[test]
fn overview_exposes_a_linked_attention_posture() {
    let template = include_str!("../templates/overview.html");
    assert!(template.contains("Attention posture"));
    assert!(template.contains("overview-attention-card"));
    assert!(template.contains("/incidents?sla=breached"));
    assert!(template.contains("/actions?outcome=review"));
    assert!(template.contains("Review posture"));
    assert!(template.contains("/actions?review=stale"));
    assert!(template.contains("/ontology?risk={{ metric.key }}"));
}

#[test]
fn overview_exposes_an_executive_operating_brief() {
    let template = include_str!("../templates/overview.html");
    assert!(template.contains("Operating brief"));
    assert!(template.contains("Signal velocity"));
    assert!(template.contains("Ownership coverage"));
    assert!(template.contains("Response readiness"));
    assert!(template.contains("/work?focus=review"));
    assert!(template.contains("Data readiness"));
    assert!(template.contains("normalized_records"));
    assert!(template.contains("/data?normalized=false"));
}

#[test]
fn signal_trend_chart_preserves_daily_drillthroughs() {
    let chart = signal_trend_chart_json(&[
        crate::events_client::DailyCount { date: "2026-07-18".into(), count: 3 },
        crate::events_client::DailyCount { date: "2026-07-19".into(), count: 0 },
    ]);
    let value: serde_json::Value = serde_json::from_str(&chart).expect("valid chart JSON");
    assert_eq!(value["labels"][0], "2026-07-18");
    assert_eq!(value["values"][1], 0);
    assert_eq!(value["hrefs"][0], "/events?from=2026-07-18&to=2026-07-18");
}

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
        incidents_client: Arc::new(crate::incidents_client::incidents_client_test::InMemoryIncidentsClient::default()),
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
        ingestion_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        egress_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
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
    assert!(body.contains("data-overview-live-status"));
    assert!(body.contains("data-overview-refresh"));
    assert!(body.contains("data-overview-toggle-live"));
    assert!(body.contains("kizashi.overview.live-refresh"));
    assert!(!body.contains("critical · <a href=\"/incidents\">"));
    assert!(!body.contains("needs review · <a href=\"/actions\">"));
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
async fn decision_queue_excludes_resolved_cases_and_prioritizes_critical_work() {
    let (mut state, session_id, tenant_id) = state_with_session().await;
    let incidents_client = Arc::new(
        crate::incidents_client::incidents_client_test::InMemoryIncidentsClient::default(),
    );
    let now = chrono::Utc::now();
    incidents_client.incidents.lock().unwrap().extend([
        crate::IncidentDetail {
            incident: Incident {
                id: Uuid::new_v4(),
                tenant_id,
                title: "resolved case".into(),
                summary: String::new(),
                severity: IncidentSeverity::Critical,
                status: IncidentStatus::Resolved,
                assigned_to: None,
                created_at: now,
                updated_at: now,
                resolved_at: Some(now),
            },
            event_ids: vec![],
            notes: vec![],
        },
        crate::IncidentDetail {
            incident: Incident {
                id: Uuid::new_v4(),
                tenant_id,
                title: "open critical case".into(),
                summary: String::new(),
                severity: IncidentSeverity::Critical,
                status: IncidentStatus::Open,
                assigned_to: None,
                created_at: now,
                updated_at: now,
                resolved_at: None,
            },
            event_ids: vec![],
            notes: vec![],
        },
    ]);
    state.incidents_client = incidents_client;

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

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("open critical case"));
    assert!(!body.contains("resolved case"));
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

#[test]
fn overview_governed_decisions_link_to_modeled_targets() {
    let source = include_str!("overview_handler.rs");
    assert!(source.contains("struct OverviewActionTarget"));
    let template = include_str!("../templates/overview.html");
    assert!(template.contains("/ontology?object_id={{ target.id }}#object-{{ target.id }}"));
    assert!(template.contains("action.targets.is_empty()"));
}
