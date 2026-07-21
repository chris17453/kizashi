use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/normalization-mappings/:id/delete", post(post_delete_normalization_mapping))
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
        incidents_client: Arc::new(
            crate::incidents_client::incidents_client_test::InMemoryIncidentsClient::default(),
        ),
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
        normalization_mappings_client: std::sync::Arc::new(InMemoryNormalizationMappingsClient::default()),
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

#[tokio::test]
async fn deleting_a_mapping_calls_the_client_and_redirects() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;
    let mapping_id = Uuid::new_v4();
    let mappings_client = Arc::new(InMemoryNormalizationMappingsClient::default());
    state.normalization_mappings_client = mappings_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/normalization-mappings/{mapping_id}/delete"))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/normalization-mappings");
    let deleted = mappings_client.deleted.lock().unwrap();
    assert_eq!(deleted.as_slice(), [mapping_id]);
}

#[tokio::test]
async fn viewer_role_cannot_delete_a_mapping() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Viewer).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/normalization-mappings/{}/delete", Uuid::new_v4()))
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/normalization-mappings/{}/delete", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}
