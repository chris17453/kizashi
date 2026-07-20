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
use crate::mfa_client::mfa_client_test::{FailingMfaClient, InMemoryMfaClient};
use crate::mfa_client::{MfaClient, MfaEnrollment};
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
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/security/mfa", get(get_mfa_settings))
        .route("/security/mfa/enroll", post(post_mfa_enroll))
        .route("/security/mfa/verify", post(post_mfa_verify))
        .route("/security/mfa/disable", post(post_mfa_disable))
        .with_state(state)
}

async fn state_with_mfa_client(mfa_client: Arc<dyn MfaClient>) -> (AppState, String) {
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
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        mfa_client,
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
    };
    (state, session_id)
}

#[tokio::test]
async fn shows_not_enabled_by_default() {
    let (state, session_id) = state_with_mfa_client(Arc::new(InMemoryMfaClient::default())).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/mfa")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("not enabled"));
}

#[tokio::test]
async fn shows_enabled_when_the_backend_reports_it() {
    let mfa_client = InMemoryMfaClient::default();
    *mfa_client.status_result.lock().unwrap() = true;
    let (state, session_id) = state_with_mfa_client(Arc::new(mfa_client)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/mfa")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("is enabled"));
}

#[tokio::test]
async fn enroll_renders_the_qr_code_and_secret() {
    let mfa_client = InMemoryMfaClient::default();
    *mfa_client.enroll_result.lock().unwrap() = Some(MfaEnrollment {
        secret_base32: "SECRETBASE32".to_string(),
        provisioning_uri: "otpauth://totp/Kizashi:alice".to_string(),
        qr_code_base64_png: "aGVsbG8=".to_string(),
    });
    let (state, session_id) = state_with_mfa_client(Arc::new(mfa_client)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/mfa/enroll")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("SECRETBASE32"));
    assert!(body.contains("aGVsbG8="));
}

#[tokio::test]
async fn verify_with_the_correct_code_redirects_to_the_settings_page() {
    let (state, session_id) = state_with_mfa_client(Arc::new(InMemoryMfaClient::default())).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/mfa/verify")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("code=123456"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/security/mfa");
}

#[tokio::test]
async fn verify_with_the_wrong_code_shows_an_error() {
    let mfa_client = InMemoryMfaClient::default();
    *mfa_client.verify_should_fail.lock().unwrap() = true;
    let (state, session_id) = state_with_mfa_client(Arc::new(mfa_client)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/mfa/verify")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("code=000000"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Incorrect code"));
}

#[tokio::test]
async fn disable_with_the_correct_password_redirects_to_the_settings_page() {
    let (state, session_id) = state_with_mfa_client(Arc::new(InMemoryMfaClient::default())).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/mfa/disable")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("password=correct-password"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn disable_with_the_wrong_password_shows_an_error() {
    let mfa_client = InMemoryMfaClient::default();
    *mfa_client.disable_should_fail.lock().unwrap() = true;
    let (state, session_id) = state_with_mfa_client(Arc::new(mfa_client)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/security/mfa/disable")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("password=wrong-password"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id) = state_with_mfa_client(Arc::new(InMemoryMfaClient::default())).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/security/mfa").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn status_backend_failure_defaults_to_not_enabled_rather_than_erroring() {
    let (state, session_id) = state_with_mfa_client(Arc::new(FailingMfaClient)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/mfa")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
