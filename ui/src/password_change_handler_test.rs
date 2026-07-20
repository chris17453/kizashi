use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::InMemoryBacklogClient;
use crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient;
use crate::branding_client::branding_client_test::InMemoryBrandingClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::oidc_client::oidc_client_test::InMemoryOidcClient;
use crate::pending_oidc_flow::InMemoryPendingOidcFlowStore;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
use crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use crate::users_client::users_client_test::InMemoryUsersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/security/password", get(get_password_settings).post(post_password_settings))
        .with_state(state)
}

async fn state_with_session() -> (AppState, String) {
    let session_store = InMemorySessionStore::default();
    let session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id: Uuid::new_v4(),
            username: "alice".to_string(),
            role: common::Role::Viewer,
            created_at: chrono::Utc::now(),
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(InMemoryBrandingClient::default()),
        oidc_client: Arc::new(InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        sensors_client: Arc::new(InMemorySensorsClient::default()),
        api_keys_client: Arc::new(InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(InMemoryBacklogClient::default()),
        execution_client: Arc::new(InMemoryExecutionClient::default()),
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        normalization_mappings_client: Arc::new(InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        ingestion_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        egress_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        login_attempts_client: Arc::new(InMemoryLoginAttemptsClient::default()),
        backup_status_client: Arc::new(InMemoryBackupStatusClient::default()),
    };
    (state, session_id)
}

fn cookie(session_id: &str) -> String {
    format!("{}={}", crate::SESSION_COOKIE_NAME, session_id)
}

#[tokio::test]
async fn get_shows_the_form() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/password")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Change Password"));
}

#[tokio::test]
async fn get_shows_a_success_banner_when_changed_query_is_set() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/password?changed=true")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Password changed successfully"));
}

#[tokio::test]
async fn post_with_matching_new_and_confirm_password_redirects() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/password")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "current_password=old-password&new_password=a-new-password-99&confirm_password=a-new-password-99",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn post_with_mismatched_confirmation_shows_an_error_without_calling_the_backend() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/password")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "current_password=old-password&new_password=a-new-password-99&confirm_password=does-not-match",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("do not match"));
}

#[tokio::test]
async fn post_with_the_wrong_current_password_surfaces_the_backends_error() {
    let (state, session_id) = state_with_session().await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/password")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "current_password=wrong-password&new_password=a-new-password-99&confirm_password=a-new-password-99",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("current password is incorrect"));
}

#[tokio::test]
async fn get_redirects_when_not_signed_in() {
    let (state, _) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/security/password").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
