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
        .route("/incidents/:id", get(get_incident_detail).post(post_update_incident))
        .route("/incidents/:id/events/:event_id/unlink", post(post_unlink_event))
        .route("/events/create-incident", post(post_create_incident_from_events))
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

fn sample_incident_detail(tenant_id: Uuid) -> crate::IncidentDetail {
    crate::IncidentDetail {
        incident: Incident {
            id: Uuid::new_v4(),
            tenant_id,
            title: "elevated error rate".to_string(),
            summary: String::new(),
            severity: IncidentSeverity::High,
            status: IncidentStatus::Open,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            resolved_at: None,
        },
        event_ids: vec![],
    }
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
    assert!(body.contains("elevated error rate"));
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
        record_ids: vec![],
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
