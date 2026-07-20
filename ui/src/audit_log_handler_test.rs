use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::audit_log_client::audit_log_client_test::{
    FailingAuditLogClient, InMemoryAuditLogClient,
};
use crate::audit_log_client::AuditLogEntry;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
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
    Router::new().route("/audit-log/:service/:entity_id", get(get_audit_log)).with_state(state)
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
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        normalization_mappings_client: Arc::new(InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        users_client: Arc::new(
            crate::users_client::users_client_test::InMemoryUsersClient::default(),
        ),
        saved_search_queries_client: Arc::new(
            crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default(),
        ),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

fn sample_entry(entity_id: Uuid) -> AuditLogEntry {
    AuditLogEntry {
        id: Uuid::new_v4(),
        entity_type: "trigger_definition".to_string(),
        entity_id,
        change_type: "created".to_string(),
        actor: "some-tenant".to_string(),
        before: None,
        after: serde_json::json!({"name": "high-volume"}),
        changed_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn shows_an_empty_state_with_no_history() {
    let (state, session_id, _tenant_id) = state_with_session().await;
    let entity_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/audit-log/config/{entity_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No audit history"));
}

#[tokio::test]
async fn shows_entries_from_the_config_client_for_the_config_service() {
    let (state, session_id, _tenant_id) = state_with_session().await;
    let entity_id = Uuid::new_v4();
    let config_client = Arc::new(InMemoryAuditLogClient::default());
    config_client.entries.lock().unwrap().insert(entity_id, vec![sample_entry(entity_id)]);
    let mut state = state;
    state.config_audit_log_client = config_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/audit-log/config/{entity_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("created"));
    assert!(body.contains("high-volume"));
}

#[tokio::test]
async fn shows_entries_from_the_retention_client_for_the_retention_service() {
    let (state, session_id, _tenant_id) = state_with_session().await;
    let entity_id = Uuid::new_v4();
    let retention_client = Arc::new(InMemoryAuditLogClient::default());
    retention_client.entries.lock().unwrap().insert(entity_id, vec![sample_entry(entity_id)]);
    let mut state = state;
    state.retention_audit_log_client = retention_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/audit-log/retention/{entity_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("created"));
}

#[tokio::test]
async fn unknown_service_shows_an_error() {
    let (state, session_id, _tenant_id) = state_with_session().await;
    let entity_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/audit-log/bogus/{entity_id}"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("unknown audit log service"));
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id, _tenant_id) = state_with_session().await;
    state.config_audit_log_client = Arc::new(FailingAuditLogClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/audit-log/config/{}", Uuid::new_v4()))
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
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;
    let entity_id = Uuid::new_v4();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/audit-log/config/{entity_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
