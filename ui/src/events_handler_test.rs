use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::session::SessionStore;
use crate::session::{InMemorySessionStore, Session};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/events", get(get_events))
        .route("/events/export.csv", get(get_events_export_csv))
        .route("/events/bulk-status", post(post_bulk_event_status))
        .with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
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
        sensors_client: Arc::new(crate::sensors_client::sensors_client_test::InMemorySensorsClient::default()),
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
        ingestion_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        egress_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
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
        record_ids: vec![],
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
    assert!(
        body.contains(r#"scope="col""#),
        "table headers should carry scope=\"col\" for screen readers"
    );
    assert!(body.contains("Create Incident from Selected"));
    assert!(body.contains(r#"name="ids""#));
    assert!(body.contains("event-trend-chart-data"));
    assert!(body.contains(r#"data-chart-kind="line""#));
}

#[test]
fn event_trend_chart_uses_daily_filters_and_drillthroughs() {
    let events = vec![
        EventSummary {
            id: Uuid::new_v4(),
            event_type: "risk.alert".to_string(),
            group_key: "customer-1".to_string(),
            status: "triggered".to_string(),
            occurred_at: "2026-07-18T12:00:00Z".parse().unwrap(),
            record_ids: vec![],
        },
        EventSummary {
            id: Uuid::new_v4(),
            event_type: "risk.alert".to_string(),
            group_key: "customer-2".to_string(),
            status: "triggered".to_string(),
            occurred_at: "2026-07-18T13:00:00Z".parse().unwrap(),
            record_ids: vec![],
        },
        EventSummary {
            id: Uuid::new_v4(),
            event_type: "risk.alert".to_string(),
            group_key: "customer-3".to_string(),
            status: "triggered".to_string(),
            occurred_at: "2026-07-19T13:00:00Z".parse().unwrap(),
            record_ids: vec![],
        },
    ];
    let query = EventsQuery {
        q: "risk".to_string(),
        status: "triggered".to_string(),
        case_scope: "unlinked".to_string(),
        ..EventsQuery::default()
    };
    let chart = event_trend_chart_json(&events, &query);
    assert!(chart.contains(r#""labels":["2026-07-18","2026-07-19"]"#));
    assert!(chart.contains(r#""values":[2,1]"#));
    assert!(chart.contains("from=2026-07-18"));
    assert!(chart.contains("q=risk"));
    assert!(chart.contains("status=triggered"));
    assert!(chart.contains("case_scope=unlinked"));
}

#[tokio::test]
async fn viewer_role_does_not_see_the_bulk_incident_checkboxes() {
    let (mut state, _admin_session_id) = state_with_session().await;
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
            username: "viewer".to_string(),
            role: common::Role::Viewer,
            created_at: chrono::Utc::now(),
        })
        .await;
    state.session_store = Arc::new(session_store);
    let events_client = InMemoryEventsClient::default();
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "sentiment_spike".to_string(),
        group_key: "customer-42".to_string(),
        status: "open".to_string(),
        occurred_at: "2026-07-18T00:00:00Z".parse().unwrap(),
        record_ids: vec![],
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
    assert!(!body.contains("Create Incident from Selected"));
    assert!(!body.contains(r#"name="ids""#));
}

#[tokio::test]
async fn an_event_with_one_contributing_record_links_straight_to_its_journey() {
    let (mut state, session_id) = state_with_session().await;
    let record_id = Uuid::new_v4();
    let events_client = InMemoryEventsClient::default();
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "sentiment_spike".to_string(),
        group_key: "customer-42".to_string(),
        status: "open".to_string(),
        occurred_at: "2026-07-18T00:00:00Z".parse().unwrap(),
        record_ids: vec![record_id],
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

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(&format!("/data/{record_id}/journey")));
    assert!(body.contains("View journey"));
}

#[tokio::test]
async fn an_event_with_multiple_contributing_records_links_to_each_journey() {
    let (mut state, session_id) = state_with_session().await;
    let record_a = Uuid::new_v4();
    let record_b = Uuid::new_v4();
    let events_client = InMemoryEventsClient::default();
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "correlated_spike".to_string(),
        group_key: "customer-42".to_string(),
        status: "open".to_string(),
        occurred_at: "2026-07-18T00:00:00Z".parse().unwrap(),
        record_ids: vec![record_a, record_b],
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

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(&format!("/data/{record_a}/journey")));
    assert!(body.contains(&format!("/data/{record_b}/journey")));
}

#[tokio::test]
async fn an_event_with_no_contributing_records_shows_a_dash_not_a_broken_link() {
    let (mut state, session_id) = state_with_session().await;
    let events_client = InMemoryEventsClient::default();
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "legacy_event".to_string(),
        group_key: "customer-42".to_string(),
        status: "open".to_string(),
        occurred_at: "2026-07-18T00:00:00Z".parse().unwrap(),
        record_ids: vec![],
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

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("/journey\""));
}

#[test]
fn parse_date_range_treats_from_as_start_of_day_and_to_as_end_of_day() {
    let (from, to) = parse_date_range("2026-07-15", "2026-07-20");
    assert_eq!(from.unwrap().to_rfc3339(), "2026-07-15T00:00:00+00:00");
    assert_eq!(to.unwrap().to_rfc3339(), "2026-07-20T23:59:59+00:00");
}

#[test]
fn parse_date_range_leaves_an_empty_or_unparseable_side_as_none() {
    let (from, to) = parse_date_range("", "not-a-date");
    assert!(from.is_none());
    assert!(to.is_none());
}

#[tokio::test]
async fn export_csv_returns_every_event_as_csv() {
    let (mut state, session_id) = state_with_session().await;
    let events_client = InMemoryEventsClient::default();
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "sentiment_spike".to_string(),
        group_key: "customer-42".to_string(),
        status: "open".to_string(),
        occurred_at: "2026-07-18T00:00:00Z".parse().unwrap(),
        record_ids: vec![],
    });
    state.events_client = Arc::new(events_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/events/export.csv")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap().to_str().unwrap(), "text/csv");
    let disposition = response.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(disposition.contains("events-"));
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.starts_with("occurred_at,event_type,group_key,status\n"));
    assert!(body.contains("sentiment_spike"));
    assert!(body.contains("customer-42"));
}

#[tokio::test]
async fn export_csv_requires_a_session() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/events/export.csv").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn bulk_status_redirect_preserves_the_active_signal_scope_when_input_is_invalid() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/bulk-status")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "target_status=invalid&q=customer%2F42&status=new&case_scope=unlinked&from=2026-07-01&to=2026-07-23&sort=occurred_at&dir=desc",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("q=customer%2F42"));
    assert!(location.contains("status=new"));
    assert!(location.contains("case_scope=unlinked"));
    assert!(location.contains("from=2026-07-01"));
    assert!(location.contains("to=2026-07-23"));
    assert!(location.contains("notice=invalid-status"));
}

#[test]
fn event_console_supports_case_scoped_signal_attachment() {
    let source = include_str!("../templates/events.html");
    assert!(source.contains("Select signals below to attach them"));
    assert!(source.contains("{% if incident.selected %} selected{% endif %}"));
    let incident_template = include_str!("../templates/incident_detail.html");
    assert!(incident_template.contains("/events?linked_incident={{ inc.id }}"));
}

#[test]
fn event_queue_keeps_source_lineage_visible_for_case_linked_signals() {
    let source = include_str!("../templates/events.html");
    assert!(source.contains("Source evidence"));
    assert!(source.contains("record journeys"));
    assert!(source.contains("No case linked"));
    assert!(source.contains("/data/{{ record_id }}/journey"));
}

#[test]
fn event_batch_controls_expose_linkage_aware_preflight() {
    let source = include_str!("../templates/events.html");
    assert!(source.contains("event-selection-preflight"));
    assert!(source.contains("data-event-linked"));
    assert!(source.contains("Linked signals remain associated"));
}

#[test]
fn event_view_save_feedback_preserves_signal_scope() {
    let source = include_str!("events_handler.rs");
    assert!(source.contains("fn event_view_redirect"));
    let template = include_str!("../templates/events.html");
    assert!(template.contains("Event investigation view saved for this exact signal scope."));
    assert!(template.contains("active signal scope is still intact."));
}
