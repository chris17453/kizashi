use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::{
    FailingEgressAllowlistClient, InMemoryEgressAllowlistClient,
};
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
    Router::new()
        .route("/egress-allowlist", get(get_egress_allowlist).post(post_egress_allowlist))
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
        analysis_config_client: Arc::new(InMemoryAnalysisConfigClient::default()),
        stats_client: Arc::new(InMemoryIngestionStatsClient::default()),
        normalization_mappings_client: Arc::new(InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        retention_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        auth_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        ingestion_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        egress_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
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

#[tokio::test]
async fn shows_an_empty_allowlist_by_default() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Admin).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/egress-allowlist")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("no restriction") || body.contains("Egress"));
}

#[tokio::test]
async fn shows_a_link_to_the_audit_history_scoped_to_the_tenant() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Admin).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/egress-allowlist")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(&format!("/audit-log/egress/{tenant_id}")));
}

#[tokio::test]
async fn post_saves_the_allowlist_and_shows_it() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let allowlist_client = Arc::new(InMemoryEgressAllowlistClient::default());
    let mut state = state;
    state.egress_allowlist_client = allowlist_client.clone();

    let body = "domains=zendesk.com%0Aapi.github.com";

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/egress-allowlist")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    {
        let saved = allowlist_client.domains.lock().unwrap();
        assert_eq!(
            saved.get(&tenant_id),
            Some(&vec!["zendesk.com".to_string(), "api.github.com".to_string()])
        );
    }
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body_text.contains("zendesk.com"));
}

#[tokio::test]
async fn post_with_a_blank_textarea_saves_an_empty_allowlist() {
    let (state, session_id, tenant_id) = state_with_session(common::Role::Operator).await;
    let allowlist_client = Arc::new(InMemoryEgressAllowlistClient::default());
    let mut state = state;
    state.egress_allowlist_client = allowlist_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/egress-allowlist")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("domains="))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let saved = allowlist_client.domains.lock().unwrap();
    assert_eq!(saved.get(&tenant_id), Some(&vec![]));
}

#[tokio::test]
async fn post_rejects_a_viewer_role() {
    let (state, session_id, _tenant_id) = state_with_session(common::Role::Viewer).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/egress-allowlist")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("domains=zendesk.com"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn shows_an_error_when_the_backend_fails() {
    let (mut state, session_id, _tenant_id) = state_with_session(common::Role::Admin).await;
    state.egress_allowlist_client = Arc::new(FailingEgressAllowlistClient);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/egress-allowlist")
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
    let (state, _session_id, _tenant_id) = state_with_session(common::Role::Admin).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/egress-allowlist").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
