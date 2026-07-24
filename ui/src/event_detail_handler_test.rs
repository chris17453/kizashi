use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::{
    FailingExecutionClient, InMemoryExecutionClient,
};
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::ActionExecutionSummary;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/events/:id", get(get_event_detail)).with_state(state)
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
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
        login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
        backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
    };
    (state, session_id, tenant_id)
}

fn sample_event(id: Uuid, record_ids: Vec<Uuid>) -> EventDetail {
    EventDetail {
        id,
        event_type: "sentiment_spike".to_string(),
        source_connector_ids: vec!["zendesk-1".to_string()],
        entity_ref: "cust-42".to_string(),
        group_key: "customer-42".to_string(),
        payload: serde_json::json!({"score": -0.8}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: "triggered".to_string(),
        record_ids,
    }
}

#[test]
fn event_object_matching_uses_source_lineage_when_entity_ref_differs() {
    let record_id = Uuid::new_v4();
    let object = common::ontology::Object {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        object_type_id: Uuid::new_v4(),
        properties: serde_json::json!({"name": "Northwind"}),
        source_lineage: serde_json::json!([record_id]),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(object_matches_event(&object, "entity-ref-from-signal", &[record_id]));
    assert!(!object_matches_event(&object, "other-entity", &[Uuid::new_v4()]));
}

#[tokio::test]
async fn renders_event_detail_with_payload() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let event_id = Uuid::new_v4();
    let events_client = Arc::new(InMemoryEventsClient::default());
    *events_client.event_detail.lock().unwrap() = Some(sample_event(event_id, vec![]));
    state.events_client = events_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/events/{event_id}"))
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
    assert!(body.contains("cust-42"));
    assert!(body.contains("-0.8"));
    assert!(body.contains("Source records"));
    assert!(body.contains("Modeled handoff"));
    assert!(body.contains("Evidence-to-action lineage"));
    assert!(body.contains("event-lineage"));
}

#[tokio::test]
async fn shows_an_error_when_the_event_is_not_found() {
    let (state, session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/events/{}", Uuid::new_v4()))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("no event found"));
}

#[tokio::test]
async fn shows_contributing_records_and_their_executions_in_the_timeline() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let event_id = Uuid::new_v4();
    let record_id = Uuid::new_v4();

    let events_client = Arc::new(InMemoryEventsClient::default());
    *events_client.event_detail.lock().unwrap() = Some(sample_event(event_id, vec![record_id]));
    state.events_client = events_client;

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
                .uri(format!("/events/{event_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Event fired"));
    assert!(body.contains("Action: webhook"));
    assert!(body.contains(&record_id.to_string()));
    assert!(body.contains("View journey"));
    assert!(body.contains("Source records"));
    assert!(body.contains("Response executions"));
}

#[tokio::test]
async fn still_renders_when_the_execution_client_fails() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    let event_id = Uuid::new_v4();

    let events_client = Arc::new(InMemoryEventsClient::default());
    *events_client.event_detail.lock().unwrap() = Some(sample_event(event_id, vec![]));
    state.events_client = events_client;
    state.execution_client = Arc::new(FailingExecutionClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/events/{event_id}"))
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
    assert!(body.contains("Event fired"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/events/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[test]
fn event_response_exposes_case_handoff_preflight() {
    let template = include_str!("../templates/event_detail.html");
    assert!(template.contains("event-response-preflight"));
    assert!(template.contains("original evidence remains attached and auditable"));
    assert!(template.contains("no source payload changes occur"));
}
