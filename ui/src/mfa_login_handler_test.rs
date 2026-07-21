use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::backlog_client::backlog_client_test::InMemoryBacklogClient;
use crate::branding_client::branding_client_test::InMemoryBrandingClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::mfa_client::mfa_client_test::InMemoryMfaClient;
use crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient;
use crate::oidc_client::oidc_client_test::InMemoryOidcClient;
use crate::pending_oidc_flow::InMemoryPendingOidcFlowStore;
use crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient;
use crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::InMemorySessionStore;
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
        .route("/login/mfa", get(get_mfa_challenge).post(post_mfa_challenge))
        .with_state(state)
}

fn state_with_mfa_client(mfa_client: InMemoryMfaClient) -> AppState {
    AppState {
        session_store: Arc::new(InMemorySessionStore::default()),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(InMemoryBrandingClient::default()),
        oidc_client: Arc::new(InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        incidents_client: Arc::new(crate::incidents_client::incidents_client_test::InMemoryIncidentsClient::default()),
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
        mfa_client: Arc::new(mfa_client),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
}
}

#[tokio::test]
async fn get_redirects_to_login_when_the_challenge_cookie_is_missing() {
    let state = state_with_mfa_client(InMemoryMfaClient::default());

    let response = router(state)
        .oneshot(Request::builder().uri("/login/mfa").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn get_renders_the_code_form_when_the_challenge_cookie_is_present() {
    let state = state_with_mfa_client(InMemoryMfaClient::default());

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/login/mfa")
                .header("cookie", "kizashi_mfa_challenge=some-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn post_with_the_correct_code_establishes_a_session() {
    let mfa_client = InMemoryMfaClient::default();
    let tenant_id = Uuid::new_v4();
    *mfa_client.challenge_result.lock().unwrap() =
        Some(("issued-token".to_string(), tenant_id, common::Role::Operator));
    let state = state_with_mfa_client(mfa_client);

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login/mfa")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("cookie", "kizashi_mfa_challenge=valid-token; kizashi_mfa_username=alice")
                .body(Body::from("code=123456"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/overview");
    let cookies: Vec<_> = response.headers().get_all("set-cookie").iter().collect();
    assert!(cookies.iter().any(|c| c.to_str().unwrap().starts_with("kizashi_session=")));
    assert!(cookies.iter().any(|c| c.to_str().unwrap().starts_with("kizashi_mfa_challenge=; ")));
}

#[tokio::test]
async fn post_with_the_wrong_code_shows_an_error_and_does_not_establish_a_session() {
    let state = state_with_mfa_client(InMemoryMfaClient::default());

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login/mfa")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("cookie", "kizashi_mfa_challenge=valid-token; kizashi_mfa_username=alice")
                .body(Body::from("code=000000"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_none());
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Invalid or expired code"));
}

#[tokio::test]
async fn post_without_a_challenge_cookie_redirects_to_login() {
    let state = state_with_mfa_client(InMemoryMfaClient::default());

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login/mfa")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("code=123456"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/login");
}
