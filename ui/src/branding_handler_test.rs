use super::*;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::branding_client::branding_client_test::InMemoryBrandingClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::oidc_client::oidc_client_test::InMemoryOidcClient;
use crate::pending_oidc_flow::InMemoryPendingOidcFlowStore;
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
    Router::new().route("/branding", get(get_branding_page).post(post_branding)).with_state(state)
}

async fn state_with_session_role(role: common::Role) -> (AppState, String, Uuid) {
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
        branding_client: Arc::new(InMemoryBrandingClient::default()),
        oidc_client: Arc::new(InMemoryOidcClient::default()),
        pending_oidc_flow_store: Arc::new(InMemoryPendingOidcFlowStore::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        incidents_client: Arc::new(crate::incidents_client::incidents_client_test::InMemoryIncidentsClient::default()),
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
        execution_client: Arc::new(
            crate::execution_client::execution_client_test::InMemoryExecutionClient::default(),
        ),
        analysis_config_client: Arc::new(crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient::default()),
        normalization_mappings_client: Arc::new(crate::normalization_mappings_client::normalization_mappings_client_test::InMemoryNormalizationMappingsClient::default()),
        retention_policies_client: Arc::new(crate::retention_policies_client::retention_policies_client_test::InMemoryRetentionPoliciesClient::default()),
        egress_allowlist_client: Arc::new(crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient::default()),
        config_audit_log_client: Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default()),
        ingestion_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        egress_audit_log_client: Arc::new(
            crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient::default(),
        ),
        users_client: Arc::new(crate::users_client::users_client_test::InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        stats_client: Arc::new(crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn get_branding_page_renders_current_values() {
    let (mut state, session_id, _tenant_id) = state_with_session_role(common::Role::Admin).await;
    let branding_client = Arc::new(InMemoryBrandingClient::default());
    *branding_client.branding.lock().unwrap() = Some(crate::branding_client::Branding {
        product_name: Some("Acme Signals".to_string()),
        logo_url: None,
        accent_color: Some("#ff6600".to_string()),
    });
    state.branding_client = branding_client;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/branding")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Acme Signals"));
    assert!(body.contains("#ff6600"));
}

#[tokio::test]
async fn get_branding_page_links_to_its_audit_history() {
    let (state, session_id, tenant_id) = state_with_session_role(common::Role::Admin).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/branding")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(&format!("/audit-log/auth/{tenant_id}")));
}

#[tokio::test]
async fn get_branding_page_hides_the_form_for_a_non_admin() {
    let (state, session_id, _tenant_id) = state_with_session_role(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/branding")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains("Your role can&#39;t edit branding")
            || body.contains("Your role can't edit branding")
    );
}

#[tokio::test]
async fn post_branding_rejects_a_non_admin() {
    let (state, session_id, _tenant_id) = state_with_session_role(common::Role::Operator).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/branding")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("product_name=Acme&logo_url=&accent_color="))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_branding_as_admin_saves_and_shows_confirmation() {
    let (mut state, session_id, tenant_id) = state_with_session_role(common::Role::Admin).await;
    let branding_client = Arc::new(InMemoryBrandingClient::default());
    state.branding_client = branding_client.clone();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/branding")
                .header("cookie", format!("kizashi_session={session_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("product_name=Acme+Signals&logo_url=&accent_color=%23ff6600"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Saved"));
    let calls = branding_client.put_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, tenant_id);
    assert_eq!(calls[0].2, "alice");
    assert_eq!(calls[0].3.product_name.as_deref(), Some("Acme Signals"));
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session_role(common::Role::Admin).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/branding").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
