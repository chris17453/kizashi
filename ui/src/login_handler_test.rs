use super::*;
use crate::auth_client::auth_client_test::{FailingAuthClient, InMemoryAuthClient};
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::session::InMemorySessionStore;
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/login", get(get_login).post(post_login)).with_state(state)
}

pub(crate) fn default_state() -> AppState {
    AppState {
        session_store: Arc::new(InMemorySessionStore::default()),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        branding_client: Arc::new(crate::branding_client::branding_client_test::InMemoryBrandingClient::default()),
        oidc_client: Arc::new(crate::oidc_client::oidc_client_test::InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(crate::pending_oidc_flow::InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
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
}
}

#[tokio::test]
async fn get_login_renders_the_form() {
    let response = router(default_state())
        .oneshot(Request::builder().uri("/login").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Sign in"));
}

#[tokio::test]
async fn get_login_with_a_tenant_name_applies_that_workspaces_branding() {
    let mut state = default_state();
    let branding_client =
        Arc::new(crate::branding_client::branding_client_test::InMemoryBrandingClient::default());
    *branding_client.branding.lock().unwrap() = Some(crate::branding_client::Branding {
        product_name: Some("Acme Signals".to_string()),
        logo_url: Some("https://acme.example.com/logo.png".to_string()),
        accent_color: Some("#ff6600".to_string()),
    });
    state.branding_client = branding_client;

    let response = router(state)
        .oneshot(Request::builder().uri("/login?tenant_name=acme").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Acme Signals"));
    assert!(body.contains("https://acme.example.com/logo.png"));
    assert!(body.contains("#ff6600"));
}

#[tokio::test]
async fn get_login_falls_back_to_defaults_when_the_workspace_has_no_branding() {
    let state = default_state();

    let response = router(state)
        .oneshot(
            Request::builder().uri("/login?tenant_name=nonexistent").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Kizashi"));
}

#[tokio::test]
async fn post_login_with_valid_credentials_sets_a_session_cookie_and_redirects() {
    let auth_client = InMemoryAuthClient::default();
    let tenant_id = Uuid::new_v4();
    *auth_client.result.lock().unwrap() = Some(LocalLoginResult::LoggedIn {
        token: "issued-token".to_string(),
        tenant_id,
        role: common::Role::Admin,
    });
    let state = AppState { auth_client: Arc::new(auth_client), ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("tenant_name=acme&username=alice&password=correct-password"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/overview");
    let set_cookie = response.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("kizashi_session="));
    assert!(set_cookie.contains("HttpOnly"));
}

#[tokio::test]
async fn post_login_with_invalid_credentials_rerenders_the_form_with_an_error() {
    let state = AppState { auth_client: Arc::new(FailingAuthClient), ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("tenant_name=acme&username=alice&password=wrong"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Invalid workspace, username, or password"));
}

#[tokio::test]
async fn post_login_with_an_unknown_workspace_rerenders_the_form_with_an_error() {
    let state =
        AppState { auth_client: Arc::new(InMemoryAuthClient::default()), ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "tenant_name=nonexistent&username=alice&password=correct-password",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Invalid workspace, username, or password"));
}
