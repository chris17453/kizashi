use super::*;

#[test]
fn reports_exposes_fast_signal_window_presets() {
    let template = include_str!("../templates/reports.html");
    assert!(template.contains("data-report-window-preset=\"1\""));
    assert!(template.contains("data-report-window-preset=\"7\""));
    assert!(template.contains("data-report-window-preset=\"30\""));
    assert!(template.contains("form.submit()"));
}

#[test]
fn report_comparison_uses_same_window_and_exposes_directional_deltas() {
    let metrics =
        report_comparison_metrics(120, Some(100), 80, Some(100), "2026-07-20", "2026-07-24");
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].delta, "+20%");
    assert_eq!(metrics[0].tone, "warning");
    assert_eq!(metrics[1].delta, "-20%");
    assert_eq!(metrics[1].tone, "good");
    assert_eq!(metrics[0].href, "/events?from=2026-07-20&to=2026-07-24");
}
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::{
    FailingIngestionStatsClient, InMemoryIngestionStatsClient,
};
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::EventSummary;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/reports", get(get_reports))
        .route("/reports/saved-views", post(post_save_report_view))
        .route("/reports/saved-views/:id/delete", post(post_delete_report_view))
        .route("/reports/export.csv", get(get_reports_export_csv))
        .route("/reports/export.pdf", get(get_reports_export_pdf))
        .with_state(state)
}

#[test]
fn previous_signal_window_is_equal_length_and_adjacent() {
    let since = Utc::now() - chrono::Duration::days(6);
    let until = Utc::now();
    let (previous_since, previous_until) = previous_signal_window(Some(since), Some(until));
    assert_eq!(previous_until.unwrap() + chrono::Duration::seconds(1), since);
    assert_eq!(
        previous_until.unwrap().signed_duration_since(previous_since.unwrap()),
        until.signed_duration_since(since)
    );
}

#[test]
fn model_scope_accepts_source_lineage_from_the_selected_window() {
    let record_id = Uuid::new_v4();
    let object = common::Object {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        object_type_id: Uuid::new_v4(),
        properties: serde_json::json!({"name": "Northwind"}),
        source_lineage: serde_json::json!([record_id]),
        created_at: "2026-06-01T00:00:00Z".parse().unwrap(),
        updated_at: "2026-06-01T00:00:00Z".parse().unwrap(),
    };
    let source_records = std::collections::HashSet::from([record_id]);
    let since = "2026-07-01T00:00:00Z".parse().unwrap();
    let until = "2026-07-23T23:59:59Z".parse().unwrap();
    assert!(object_in_window(&object, &source_records, Some(since), Some(until)));
    assert!(!object_in_window(
        &object,
        &std::collections::HashSet::new(),
        Some(since),
        Some(until)
    ));
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
    (state, session_id)
}

#[tokio::test]
async fn renders_connector_stats_and_event_counts_when_signed_in() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.stats.lock().unwrap().push(ConnectorStatSummary {
        connector_id: "zendesk".to_string(),
        record_count: 7,
        last_ingested_at: chrono::Utc::now(),
    });
    state.stats_client = stats_client;
    let events_client = Arc::new(InMemoryEventsClient::default());
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "sentiment".to_string(),
        group_key: "cust-1".to_string(),
        status: "open".to_string(),
        occurred_at: chrono::Utc::now(),
        record_ids: vec![],
    });
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/reports")
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
    assert!(body.contains("sentiment"));
}

#[test]
fn executive_kpis_do_not_nest_interactive_links() {
    let template = include_str!("../templates/reports.html");
    assert!(!template.contains("needs review · <a href=\"/actions\">"));
    assert!(template.contains("/ontology?object_id={{ target.id }}#object-{{ target.id }}"));
    assert!(template.contains("/actions/{{ action.id }}"));
}

#[test]
fn report_incident_rows_link_window_events_into_evidence() {
    let source = include_str!("reports_handler.rs");
    assert!(source.contains("event_labels.get(id)"));
    let template = include_str!("../templates/reports.html");
    assert!(template.contains("Evidence handoff"));
    assert!(template.contains("/events/{{ event.id }}"));
}

#[test]
fn report_exposes_ontology_coverage_handoffs() {
    let source = include_str!("reports_handler.rs");
    assert!(source.contains("struct OntologyCoverageRow"));
    assert!(source.contains("object_counts"));
    assert!(source.contains("relationship_count"));
    let template = include_str!("../templates/reports.html");
    assert!(template.contains("Ontology coverage"));
    assert!(template.contains("Relationship types"));
    assert!(template.contains("/ontology?type_id={{ row.id }}"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/reports").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn shows_an_error_when_stats_backend_fails() {
    let (mut state, session_id) = state_with_session().await;
    state.stats_client = Arc::new(FailingIngestionStatsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/reports")
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
async fn chart_data_escapes_a_connector_id_that_could_close_the_script_tag() {
    let (mut state, session_id) = state_with_session().await;
    let stats_client = Arc::new(InMemoryIngestionStatsClient::default());
    stats_client.stats.lock().unwrap().push(ConnectorStatSummary {
        connector_id: "</script><script>alert(1)</script>".to_string(),
        record_count: 1,
        last_ingested_at: chrono::Utc::now(),
    });
    state.stats_client = stats_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/reports")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        !body.contains("</script><script>alert(1)</script>"),
        "the literal closing tag must never appear unescaped inside the JSON <script> block"
    );
    let expected_escaped = "{\"labels\":[\"\\u003c/script>\\u003cscript>alert(1)\\u003c/script>\"]";
    assert!(
        body.contains(expected_escaped),
        "the script-tag JSON payload must escape every '<' as \\u003c so a value containing \
         the literal text </script> can never be parsed as a real closing tag"
    );
}

#[tokio::test]
async fn report_window_is_rendered_and_export_preserves_it() {
    let (mut state, session_id) = state_with_session().await;
    let events_client = Arc::new(InMemoryEventsClient::default());
    events_client.events.lock().unwrap().push(EventSummary {
        id: Uuid::new_v4(),
        event_type: "customer.health.degraded".to_string(),
        group_key: "cust-42".to_string(),
        status: "open".to_string(),
        occurred_at: chrono::Utc::now(),
        record_ids: vec![],
    });
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/reports?from=2026-07-01&to=2026-07-23")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("value=\"2026-07-01\""));
    assert!(body.contains("value=\"2026-07-23\""));
    assert!(body.contains("Signals in window"));
    assert!(body.contains("Readiness to act"));
    assert!(body.contains("Executive decision gates"));
    assert!(body.contains("vs prior window"));
    assert!(body.contains("/reports/export.csv?from=2026-07-01"));
    assert!(body.contains("to=2026-07-23"));

    let (export_state, export_session_id) = state_with_session().await;
    let response = router(export_state)
        .oneshot(
            Request::builder()
                .uri("/reports/export.csv?from=2026-07-01&to=2026-07-23")
                .header("cookie", format!("kizashi_session={export_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/csv; charset=utf-8");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("section,signal_window"));
    assert!(body.contains("2026-07-01..2026-07-23"));

    let (pdf_state, pdf_session_id) = state_with_session().await;
    let response = router(pdf_state)
        .oneshot(
            Request::builder()
                .uri("/reports/export.pdf?from=2026-07-01&to=2026-07-23")
                .header("cookie", format!("kizashi_session={pdf_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "application/pdf");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
}

#[tokio::test]
async fn saves_and_renders_a_report_view_for_the_current_tenant() {
    let (state, session_id) = state_with_session().await;
    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/reports/saved-views")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("name=Weekly+posture&from=2026-07-01&to=2026-07-23"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "/reports?from=2026-07-01&to=2026-07-23&notice=view_saved"
    );

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/reports?from=2026-07-01&to=2026-07-23&notice=view_saved")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Weekly posture"));
    assert!(body.contains("/reports?from=2026-07-01&amp;to=2026-07-23"));
    assert!(body.contains("Report view saved for this signal window."));
}

#[test]
fn report_saved_view_feedback_preserves_the_active_window() {
    let source = include_str!("reports_handler.rs");
    assert!(source.contains("notice=view_saved"));
    assert!(source.contains("notice=view_failed"));
    assert!(source.contains("serde_urlencoded::to_string"));
    let template = include_str!("../templates/reports.html");
    assert!(template.contains("Your signal window is still intact."));
}

#[test]
fn report_matrix_handoffs_preserve_the_selected_window() {
    let template = include_str!("../templates/reports.html");
    assert!(template.contains("/incidents?from={{ from|urlencode }}&amp;to={{ to|urlencode }}&amp;severity={{ row.severity }}"));
    assert!(template.contains("&amp;status=open"));
    assert!(template.contains("&amp;sla=breached"));
}

#[test]
fn report_operating_funnel_keeps_the_window_on_each_investigation_handoff() {
    let stages = report_operating_funnel(100, 50, 10, 20, 5, "2026-07-01", "2026-07-23");

    assert_eq!(stages.len(), 5);
    assert_eq!(stages[0].label, "Evidence");
    assert_eq!(stages[0].count, 100);
    assert!(stages[0].href.contains("/data?from=2026-07-01&to=2026-07-23"));
    assert!(stages[2].href.contains("/incidents?status=active&from=2026-07-01&to=2026-07-23"));
    assert!(stages[4].href.contains("/actions?from=2026-07-01&to=2026-07-23"));
    assert_eq!(stages[4].percent, 5);
}
