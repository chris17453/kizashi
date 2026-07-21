use super::*;
use crate::api_keys_client::api_keys_client_test::InMemoryApiKeysClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient;
use crate::sensors_client::sensors_client_test::InMemorySensorsClient;
use crate::session::{InMemorySessionStore, Session, SessionStore};
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/api-keys", get(get_api_keys).post(post_api_keys))
        .route("/api-keys/:id/revoke", post(post_revoke_api_key))
        .route("/api-keys/bulk-revoke", post(post_bulk_revoke_api_keys))
        .with_state(state)
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
        api_keys_client: Arc::new(InMemoryApiKeysClient::default()),
        backlog_client: Arc::new(
            crate::backlog_client::backlog_client_test::InMemoryBacklogClient::default(),
        ),
        execution_client: std::sync::Arc::new(
            crate::execution_client::execution_client_test::InMemoryExecutionClient::default(),
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
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

#[tokio::test]
async fn viewer_role_does_not_see_create_form_or_revoke_buttons() {
    let (state, _admin_session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();
    let viewer_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "viewer-alice".to_string(),
            role: common::Role::Viewer,
            created_at: chrono::Utc::now(),
        })
        .await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("cookie", format!("kizashi_session={viewer_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("ci-agent"));
    assert!(!body.contains("Create a new key"));
    assert!(!body.contains(">Revoke<"));
}

#[tokio::test]
async fn operator_role_sees_create_form_and_revoke_buttons() {
    let (state, _admin_session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();
    let operator_session_id = state
        .session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "operator-alice".to_string(),
            role: common::Role::Operator,
            created_at: chrono::Utc::now(),
        })
        .await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("cookie", format!("kizashi_session={operator_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Create a new key"));
    assert!(body.contains(">Revoke<"));
}

#[tokio::test]
async fn get_api_keys_renders_the_table_when_signed_in() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("ci-agent"));
}

#[tokio::test]
async fn get_api_keys_revoke_buttons_ask_for_confirmation_before_submitting() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    // Both the single-key revoke form and the bulk-revoke-form (the latter targeted via a
    // button's form= attribute rather than physically wrapping the table) confirm before
    // submitting.
    assert_eq!(body.matches("onsubmit=\"return confirm(").count(), 2);
}

#[tokio::test]
async fn get_api_keys_links_each_key_to_its_audit_history() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();
    let keys = state.api_keys_client.list_api_keys(tenant_id).await.unwrap();
    let key_id = keys.iter().find(|k| k.label == "ci-agent").unwrap().id;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(&format!("/audit-log/ingestion/{key_id}")));
}

#[tokio::test]
async fn get_api_keys_filters_by_the_q_query_param_case_insensitively() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "zzz-other-key", "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys?q=CI-AGENT")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("ci-agent"));
    assert!(!body.contains("zzz-other-key"));
}

#[tokio::test]
async fn get_api_keys_sorts_by_label_descending_when_dir_is_desc() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "alpha-key", "test-actor")
        .await
        .unwrap();
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "zeta-key", "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys?sort=label&dir=desc")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let alpha_pos = body.find("alpha-key").unwrap();
    let zeta_pos = body.find("zeta-key").unwrap();
    assert!(zeta_pos < alpha_pos);
}

#[tokio::test]
async fn get_api_keys_shows_a_no_match_empty_state_for_an_unmatched_query() {
    let (state, session_id, tenant_id) = state_with_session().await;
    state
        .api_keys_client
        .create_api_key(tenant_id, common::Role::Admin, "ci-agent", "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api-keys?q=nobody-matches-this")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No API keys match"));
}

#[tokio::test]
async fn get_api_keys_redirects_to_login_when_not_signed_in() {
    let (state, _session_id, _tenant_id) = state_with_session().await;

    let response = router(state)
        .oneshot(Request::builder().uri("/api-keys").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
