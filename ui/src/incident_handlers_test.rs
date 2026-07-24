use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::events_client::EventDetail as ClientEventDetail;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::incidents_client::incidents_client_test::{
    FailingIncidentsClient, InMemoryIncidentsClient,
};
use crate::session::{InMemorySessionStore, Session, SessionStore};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/incidents", get(get_incidents).post(post_incident))
        .route("/incidents/export.csv", get(get_incidents_export_csv))
        .route("/incidents/bulk-update", post(post_bulk_update_incidents))
        .route("/incidents/:id", get(get_incident_detail).post(post_update_incident))
        .route("/incidents/:id/claim", post(post_claim_incident))
        .route("/incidents/:id/events/:event_id/unlink", post(post_unlink_event))
        .route("/events/create-incident", post(post_create_incident_from_events))
        .route("/events/link-incident", post(post_link_events_to_incident))
        .with_state(state)
}

async fn state_with_session(role: common::Role) -> (AppState, String, Uuid) {
    let session_store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "alice".to_string(),
            role,
            created_at: chrono::Utc::now(),
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(
            crate::branding_client::branding_client_test::InMemoryBrandingClient::default(),
        ),
        oidc_client: Arc::new(crate::oidc_client::oidc_client_test::InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(
            crate::pending_oidc_flow::InMemoryPendingOidcFlowStore::default(),
        ),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(
            crate::triggers_client::triggers_client_test::InMemoryTriggersClient::default(),
        ),
        incidents_client: Arc::new(InMemoryIncidentsClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        sensors_client: Arc::new(
            crate::sensors_client::sensors_client_test::InMemorySensorsClient::default(),
        ),
        api_keys_client: Arc::new(
            crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient::default(),
        ),
        backlog_client: Arc::new(
            crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default(),
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
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
        execution_client: Arc::new(
            crate::execution_client::execution_client_test::InMemoryExecutionClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
        login_attempts_client: Arc::new(
            crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default(),
        ),
        backup_status_client: Arc::new(
            crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default(),
        ),
    };
    (state, session_id, tenant_id)
}

#[test]
fn correlation_clusters_group_cases_by_shared_signal_context() {
    let shared_group = "Northwind Health".to_string();
    let incidents = vec![
        IncidentRow {
            id: Uuid::new_v4(),
            title: "First case".into(),
            summary: String::new(),
            signal_context: shared_group.clone(),
            group_keys: vec![shared_group.clone()],
            severity: common::IncidentSeverity::High,
            status: common::IncidentStatus::Open,
            assigned_to: None,
            event_count: 2,
            created_at: chrono::Utc::now(),
            sla_state: "on-track".into(),
            sla_label: "On track".into(),
            sla_detail: String::new(),
        },
        IncidentRow {
            id: Uuid::new_v4(),
            title: "Second case".into(),
            summary: String::new(),
            signal_context: shared_group.clone(),
            group_keys: vec![shared_group.clone()],
            severity: common::IncidentSeverity::Critical,
            status: common::IncidentStatus::Acknowledged,
            assigned_to: Some("alice".into()),
            event_count: 1,
            created_at: chrono::Utc::now(),
            sla_state: "at-risk".into(),
            sla_label: "At risk".into(),
            sla_detail: String::new(),
        },
    ];
    let clusters = incident_correlation_clusters(&incidents);
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].group_key, shared_group);
    assert_eq!(clusters[0].case_count, 2);
    assert_eq!(clusters[0].signal_count, 3);
    assert_eq!(clusters[0].severity, "critical");
}

fn sample_incident_detail(tenant_id: Uuid) -> crate::IncidentDetail {
    crate::IncidentDetail {
        incident: Incident {
            id: Uuid::new_v4(),
            tenant_id,
            title: "elevated error rate".to_string(),
            summary: String::new(),
            severity: IncidentSeverity::High,
            status: IncidentStatus::Open,
            assigned_to: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            resolved_at: None,
        },
        event_ids: vec![],
        notes: vec![],
    }
}

#[tokio::test]
async fn bulk_link_events_to_existing_incident_is_operator_only_and_reports_success() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let incidents = Arc::new(InMemoryIncidentsClient::default());
    let incident = sample_incident_detail(tenant_id);
    let incident_id = incident.incident.id;
    incidents.incidents.lock().unwrap().push(incident);
    state.incidents_client = incidents.clone();
    let event_a = Uuid::new_v4();
    let event_b = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/link-incident")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={event_a}&ids={event_b}&incident_id={incident_id}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("events-linked"));
    assert_eq!(incidents.linked.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn operator_can_claim_an_unassigned_incident_through_the_audited_update_path() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let incidents = Arc::new(InMemoryIncidentsClient::default());
    let incident = sample_incident_detail(tenant_id);
    let incident_id = incident.incident.id;
    incidents.incidents.lock().unwrap().push(incident);
    state.incidents_client = incidents.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/incidents/{incident_id}/claim"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/work?notice=claimed");
    assert_eq!(incidents.updated.lock().unwrap()[0].assigned_to.as_deref(), Some("alice"));
}

#[tokio::test]
async fn incidents_list_renders_real_data() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().push(sample_incident_detail(tenant_id));
    state.incidents_client = incidents_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/incidents?view=board")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("elevated error rate"));
    assert!(body.contains("1 · active"));
    assert!(body.contains("0 · in response"));
    assert!(body.contains("0 · closed"));
    assert!(body.contains("status=open"));
    assert!(body.contains("sla=breached"));
}

#[tokio::test]
async fn incident_detail_renders_immutable_activity_context() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let detail = sample_incident_detail(tenant_id);
    let incident_id = detail.incident.id;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().push(detail);
    incidents_client.audit.lock().unwrap().push(crate::audit_log_client::AuditLogEntry {
        id: Uuid::new_v4(),
        entity_type: "incident".to_string(),
        entity_id: incident_id,
        change_type: "updated".to_string(),
        actor: "alice".to_string(),
        before: Some(serde_json::json!({"status": "open"})),
        after: serde_json::json!({"status": "acknowledged"}),
        changed_at: chrono::Utc::now(),
    });
    state.incidents_client = incidents_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/incidents/{incident_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Case activity"));
    assert!(body.contains("status: open → acknowledged"));
    assert!(body.contains("alice"));
}

#[tokio::test]
async fn incident_queue_filters_by_search_and_severity() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let mut matching = sample_incident_detail(tenant_id);
    matching.incident.title = "elevated checkout error rate".to_string();
    matching.incident.severity = IncidentSeverity::Critical;
    let mut unrelated = sample_incident_detail(tenant_id);
    unrelated.incident.title = "billing reconciliation delay".to_string();
    unrelated.incident.severity = IncidentSeverity::Low;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().extend([matching, unrelated]);
    state.incidents_client = incidents_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/incidents?q=checkout&severity=critical")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("elevated checkout error rate"));
    assert!(!body.contains("billing reconciliation delay"));
}

#[tokio::test]
async fn incident_queue_filters_by_sla_posture() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let mut breached = sample_incident_detail(tenant_id);
    breached.incident.title = "breached response case".to_string();
    breached.incident.created_at = chrono::Utc::now() - chrono::Duration::hours(2);
    let mut on_track = sample_incident_detail(tenant_id);
    on_track.incident.title = "on track response case".to_string();
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().extend([breached, on_track]);
    state.incidents_client = incidents_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/incidents?sla=breached")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("breached response case"));
    assert!(!body.contains("on track response case"));
}

#[tokio::test]
async fn incident_queue_active_scope_excludes_resolved_cases() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let active = sample_incident_detail(tenant_id);
    let mut resolved = sample_incident_detail(tenant_id);
    resolved.incident.title = "resolved historical case".to_string();
    resolved.incident.status = IncidentStatus::Resolved;
    resolved.incident.resolved_at = Some(chrono::Utc::now());
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().extend([active, resolved]);
    state.incidents_client = incidents_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/incidents?status=active")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("elevated error rate"));
    assert!(!body.contains("resolved historical case"));
    assert!(body.contains("Active (open + acknowledged)"));
}

#[tokio::test]
async fn bulk_incident_update_applies_audited_lifecycle_transition() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let first = sample_incident_detail(tenant_id);
    let first_id = first.incident.id;
    let second = sample_incident_detail(tenant_id);
    let second_id = second.incident.id;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().extend([first, second]);
    state.incidents_client = incidents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/incidents/bulk-update")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "ids={first_id}&ids={second_id}&target_status=acknowledged"
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/incidents?notice=bulk_updated");
    let updated = incidents_client.updated.lock().unwrap();
    assert_eq!(updated.len(), 2);
    assert!(updated.iter().all(|incident| incident.status == IncidentStatus::Acknowledged));
}

#[tokio::test]
async fn viewer_cannot_bulk_update_incidents() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Viewer).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/incidents/bulk-update")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={}&target_status=resolved", Uuid::new_v4())))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn creating_an_incident_redirects_to_its_detail_page() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/incidents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=new+incident&severity=high"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/incidents/"));
}

#[tokio::test]
async fn viewer_role_cannot_create_an_incident() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Viewer).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/incidents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=new+incident&severity=high"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn incident_detail_shows_an_error_when_not_found() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/incidents/{}", Uuid::new_v4()))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("no incident found"));
}

#[tokio::test]
async fn incident_detail_shows_linked_events() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let event_id = Uuid::new_v4();
    let record_id = Uuid::new_v4();
    let mut detail = sample_incident_detail(tenant_id);
    detail.event_ids = vec![event_id];
    let incident_id = detail.incident.id;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().push(detail);
    state.incidents_client = incidents_client;

    let events_client = Arc::new(InMemoryEventsClient::default());
    *events_client.event_detail.lock().unwrap() = Some(ClientEventDetail {
        id: event_id,
        event_type: "sentiment_spike".to_string(),
        source_connector_ids: vec![],
        entity_ref: "cust-1".to_string(),
        group_key: "cust-1".to_string(),
        payload: serde_json::json!({}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: "triggered".to_string(),
        record_ids: vec![record_id],
    });
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/incidents/{incident_id}"))
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
    assert!(body.contains("Source evidence"));
    assert!(body.contains(&format!("/data/{record_id}/journey")));
}

#[tokio::test]
async fn updating_status_and_severity_calls_the_client() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let detail = sample_incident_detail(tenant_id);
    let incident_id = detail.incident.id;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().push(detail);
    state.incidents_client = incidents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/incidents/{incident_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=renamed&severity=critical&status=resolved"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let updated = incidents_client.updated.lock().unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].status, IncidentStatus::Resolved);
    assert_eq!(updated[0].severity, IncidentSeverity::Critical);
    assert!(updated[0].resolved_at.is_some());
}

#[tokio::test]
async fn unlinking_an_event_calls_the_client_and_redirects() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;
    let incident_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    state.incidents_client = incidents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/incidents/{incident_id}/events/{event_id}/unlink"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let unlinked = incidents_client.unlinked.lock().unwrap();
    assert_eq!(unlinked.as_slice(), [(incident_id, event_id)]);
}

#[tokio::test]
async fn create_incident_from_selected_events_links_them_and_redirects() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    state.incidents_client = incidents_client.clone();
    let event_id_1 = Uuid::new_v4();
    let event_id_2 = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create-incident")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={event_id_1}&ids={event_id_2}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/incidents/"));
    let created = incidents_client.created.lock().unwrap();
    assert_eq!(created.len(), 1);
}

#[tokio::test]
async fn correlates_selected_event_into_an_existing_open_case_by_entity_context() {
    let (mut state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let existing_event_id = Uuid::new_v4();
    let new_event_id = Uuid::new_v4();
    let mut existing_detail = sample_incident_detail(tenant_id);
    let incident_id = existing_detail.incident.id;
    existing_detail.event_ids = vec![existing_event_id];
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    incidents_client.incidents.lock().unwrap().push(existing_detail);
    state.incidents_client = incidents_client.clone();
    let events_client = Arc::new(InMemoryEventsClient::default());
    *events_client.event_detail.lock().unwrap() = Some(ClientEventDetail {
        id: new_event_id,
        event_type: "sentiment_spike".to_string(),
        source_connector_ids: vec![],
        entity_ref: "customer-42".to_string(),
        group_key: "customer-42".to_string(),
        payload: serde_json::json!({}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: "triggered".to_string(),
        record_ids: vec![],
    });
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create-incident")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={new_event_id}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap(),
        &format!("/incidents/{incident_id}?notice=correlated&linked_count=1")
    );
    assert!(incidents_client.created.lock().unwrap().is_empty());
    assert_eq!(incidents_client.linked.lock().unwrap().as_slice(), [(incident_id, new_event_id)]);
}

#[tokio::test]
async fn create_incident_from_no_selected_events_redirects_to_events_without_creating() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;
    let incidents_client = Arc::new(InMemoryIncidentsClient::default());
    state.incidents_client = incidents_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/events/create-incident")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/events");
    assert!(incidents_client.created.lock().unwrap().is_empty());
}

#[tokio::test]
async fn incidents_list_shows_a_backend_error() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;
    state.incidents_client = Arc::new(FailingIncidentsClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/incidents")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("incident service unreachable"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/incidents").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[test]
fn incident_csv_escape_quotes_case_evidence() {
    assert_eq!(csv_escape("Customer, Northwind"), "\"Customer, Northwind\"");
    assert_eq!(csv_escape("before\"after"), "\"before\"\"after\"");
}

#[test]
fn incident_sla_marks_a_high_case_breach() {
    let now = chrono::Utc::now();
    let incident = Incident {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        title: "late response".to_string(),
        summary: String::new(),
        severity: IncidentSeverity::High,
        status: IncidentStatus::Open,
        assigned_to: None,
        created_at: now - chrono::Duration::hours(2),
        updated_at: now,
        resolved_at: None,
    };
    assert_eq!(incident_sla(&incident, now).state, "breached");
}

#[test]
fn saved_incident_view_preserves_table_board_scope_filters() {
    let saved = common::SavedSearchQuery::new(
        Uuid::new_v4(),
        "Critical open board",
        serde_json::json!({"view_kind":"incidents","view":"board","status":"open","severity":"critical","owner":"alice","q":"latency"}),
    );
    let view = super::to_saved_incident_view(saved);
    assert!(view.load_url.contains("view=board"));
    assert!(view.load_url.contains("status=open"));
    assert!(view.load_url.contains("severity=critical"));
    assert!(view.load_url.contains("owner=alice"));
}

#[test]
fn incident_search_matches_brief_and_linked_signal_context() {
    assert!(incident_matches_search(
        "Payment outage",
        "Northwind checkout investigation",
        "risk.payment-timeout checkout-api open",
        "northwind"
    ));
    assert!(incident_matches_search(
        "Payment outage",
        "Northwind checkout investigation",
        "risk.payment-timeout checkout-api open",
        "payment-timeout"
    ));
    assert!(!incident_matches_search(
        "Payment outage",
        "Northwind checkout investigation",
        "risk.payment-timeout checkout-api open",
        "unrelated"
    ));
}

#[test]
fn incident_queue_navigation_preserves_sla_scope() {
    let template = include_str!("../templates/incidents.html");
    assert!(
        template.contains("sort=title&dir=")
            && template.contains("&sla={{ sla|urlencode }}&owner=")
    );
    assert!(
        template.contains("incident-pagination")
            && template.contains("&sla={{ sla|urlencode }}&owner={{ owner|urlencode }}")
    );
}

#[test]
fn board_status_redirect_preserves_operational_scope() {
    let form = super::IncidentStatusTransitionForm {
        target_status: "acknowledged".into(),
        q: "northwind".into(),
        status_filter: "open".into(),
        severity: "high".into(),
        sla: "breached".into(),
        owner: "demo".into(),
        sort: "created_at".into(),
        dir: "desc".into(),
    };
    let response = super::board_transition_redirect("transitioned", &form).into_response();
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("sla=breached"));
    assert!(location.contains("owner=demo"));
    assert!(location.contains("q=northwind"));
}

#[test]
fn incident_bulk_mutations_preserve_the_active_queue_scope() {
    let template = include_str!("../templates/incidents.html");
    assert!(template.contains("name=\"sla\" value=\"{{ sla }}\""));
    assert!(template.contains("name=\"view\" value=\"{{ view }}\""));
}

#[test]
fn incident_bulk_transition_exposes_audited_preflight() {
    let template = include_str!("../templates/incidents.html");
    assert!(template.contains("incident-bulk-preflight"));
    assert!(template.contains("Changes are audited per case."));
    assert!(template.contains("target-status"));
    assert!(template.contains("target-owner"));
}

#[test]
fn incident_response_history_includes_case_level_invocations() {
    let source = include_str!("incident_handlers.rs");
    assert!(source.contains("incident_id != Some(id)"));
    assert!(source.contains("Case-level response"));
    let template = include_str!("../templates/incident_detail.html");
    assert!(template.contains("Governed response chain"));
    assert!(template.contains("Response review posture"));
    assert!(template.contains("/actions/{{ response.id }}"));
    assert!(source.contains("list_action_reviews"));
}

#[test]
fn incident_case_actions_expose_target_preflight_boundary() {
    let template = include_str!("../templates/incident_detail.html");
    assert!(template.contains("case-action-preflight"));
    assert!(template.contains("No state changes occur until you submit."));
    assert!(template.contains("current governed preconditions"));
}

#[test]
fn incident_relationship_context_expands_two_hops_from_direct_impact() {
    let now = chrono::Utc::now();
    let ticket = uuid::Uuid::new_v4();
    let customer = uuid::Uuid::new_v4();
    let team = uuid::Uuid::new_v4();
    let ticket_type = uuid::Uuid::new_v4();
    let customer_type = uuid::Uuid::new_v4();
    let team_type = uuid::Uuid::new_v4();
    let raised_by = uuid::Uuid::new_v4();
    let supported_by = uuid::Uuid::new_v4();
    let object = |id, object_type_id, label| common::ontology::Object {
        id,
        tenant_id: uuid::Uuid::new_v4(),
        object_type_id,
        properties: serde_json::json!({"name": label}),
        source_lineage: serde_json::json!([]),
        created_at: now,
        updated_at: now,
    };
    let objects = vec![
        object(ticket, ticket_type, "Ticket 1842"),
        object(customer, customer_type, "Northwind Health"),
        object(team, team_type, "Identity Operations"),
    ];
    let link = |id, link_type_id, source_object_id, target_object_id| common::ontology::Link {
        id,
        tenant_id: uuid::Uuid::new_v4(),
        link_type_id,
        source_object_id,
        target_object_id,
        properties: None,
        created_at: now,
        updated_at: now,
    };
    let links = vec![
        link(uuid::Uuid::new_v4(), raised_by, ticket, customer),
        link(uuid::Uuid::new_v4(), supported_by, customer, team),
    ];
    let link_types = vec![
        common::ontology::LinkType {
            id: raised_by,
            tenant_id: uuid::Uuid::new_v4(),
            name: "Raised by".into(),
            source_object_type_id: ticket_type,
            target_object_type_id: customer_type,
            cardinality: "many-to-one".into(),
            properties_schema: None,
            created_at: now,
            updated_at: now,
        },
        common::ontology::LinkType {
            id: supported_by,
            tenant_id: uuid::Uuid::new_v4(),
            name: "Supported by".into(),
            source_object_type_id: customer_type,
            target_object_type_id: team_type,
            cardinality: "many-to-one".into(),
            properties_schema: None,
            created_at: now,
            updated_at: now,
        },
    ];
    let type_names = std::collections::HashMap::from([
        (ticket_type, "Support Ticket".to_string()),
        (customer_type, "Customer".to_string()),
        (team_type, "Support Team".to_string()),
    ]);
    let relationships = super::impact_relationships(
        &objects,
        &type_names,
        &link_types,
        &links,
        &std::collections::HashSet::from([ticket]),
        2,
    );
    assert_eq!(relationships.len(), 2);
    assert_eq!(relationships[1].target_label, "Identity Operations");
}

#[test]
fn incident_view_save_preserves_the_active_queue_scope() {
    let form = super::SaveIncidentViewForm {
        name: "Critical unassigned".to_string(),
        q: "Northwind".to_string(),
        status: "open".to_string(),
        severity: "critical".to_string(),
        owner: "".to_string(),
        sort: "created_at".to_string(),
        dir: "desc".to_string(),
        view: "table".to_string(),
        sla: "breached".to_string(),
    };
    let response = super::incident_view_redirect(&form, "view_saved").into_response();
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("q=Northwind"));
    assert!(location.contains("severity=critical"));
    assert!(location.contains("sla=breached"));
    assert!(location.contains("view=table"));
    assert!(location.contains("notice=view_saved"));
}
