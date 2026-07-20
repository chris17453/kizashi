use super::*;
use crate::analysis_config_client::analysis_config_client_test::{
    FailingAnalysisConfigClient, InMemoryAnalysisConfigClient,
};
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::execution_client::execution_client_test::InMemoryExecutionClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
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
    Router::new()
        .route("/analysis-config", get(get_analysis_config_page).post(post_analysis_config))
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
        normalization_mappings_client: std::sync::Arc::new(crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: std::sync::Arc::new(crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: std::sync::Arc::new(crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        retention_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        auth_audit_log_client: std::sync::Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        users_client: std::sync::Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: std::sync::Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn renders_an_empty_prompt_when_none_configured() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("AI Analysis"));
}

#[tokio::test]
async fn get_links_to_its_audit_history() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(&format!("/audit-log/config/{tenant_id}")));
}

#[tokio::test]
async fn post_saves_the_prompt_and_rerenders_it() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("prompt=look+for+urgent+tickets"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("look for urgent tickets"));
    assert!(body.contains("Saved"));
}

#[tokio::test]
async fn post_saves_an_openai_compatible_provider_config() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "prompt=flag+urgent+issues&provider=openai_compatible&model=qwen3%3A8b&endpoint=http%3A%2F%2Flocalhost%3A11434%2Fv1",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("qwen3:8b"));
    assert!(body.contains("http://localhost:11434/v1"));
}

/// RBAC-fix regression coverage: leaving the API key field blank on a follow-up save must not
/// wipe a previously-configured key, since the page can never show the real key to leave in
/// place — see `AnalysisConfigInput::api_key`'s tri-state doc comment.
#[tokio::test]
async fn post_with_a_blank_api_key_field_preserves_a_previously_configured_key() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let app = router(state.clone());

    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/analysis-config")
            .header("cookie", format!("kizashi_session={session_id}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("prompt=first&provider=openai_compatible&api_key=keep-me"))
            .unwrap(),
    )
    .await
    .unwrap();

    router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("prompt=second&provider=openai_compatible"))
                .unwrap(),
        )
        .await
        .unwrap();

    let stored = state
        .analysis_config_client
        .get_analysis_config(tenant_id)
        .await
        .unwrap()
        .expect("config should exist");
    assert_eq!(stored.api_key, Some("keep-me".to_string()));
}

/// Checking "clear the configured API key" explicitly removes it, even though a blank field
/// alone no longer does.
#[tokio::test]
async fn post_with_clear_api_key_checked_removes_a_previously_configured_key() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let app = router(state.clone());

    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/analysis-config")
            .header("cookie", format!("kizashi_session={session_id}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("prompt=first&provider=openai_compatible&api_key=clear-me"))
            .unwrap(),
    )
    .await
    .unwrap();

    router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("prompt=second&provider=openai_compatible&clear_api_key=true"))
                .unwrap(),
        )
        .await
        .unwrap();

    let stored = state
        .analysis_config_client
        .get_analysis_config(tenant_id)
        .await
        .unwrap()
        .expect("config should exist");
    assert_eq!(stored.api_key, None);
}

/// The rendered page never echoes a real API key back into the form, but does tell the operator
/// one is already configured so they know a blank field isn't the same as "no key set".
#[tokio::test]
async fn get_page_never_shows_the_real_api_key_but_flags_it_as_configured() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("prompt=x&provider=openai_compatible&api_key=top-secret-value"))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("top-secret-value"));
    assert!(body.contains("already configured"));
}

#[tokio::test]
async fn post_rejects_a_viewer_role() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Viewer).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("prompt=x"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_shows_an_error_when_the_backend_fails() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Operator).await;
    state.analysis_config_client = Arc::new(FailingAnalysisConfigClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/analysis-config")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("simulated failure") || body.contains("error"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session(common::Role::Operator).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/analysis-config").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
