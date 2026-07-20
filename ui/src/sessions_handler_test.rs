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
        .route("/security/sessions", get(get_sessions))
        .route("/security/sessions/:id/revoke", post(post_revoke_session))
        .with_state(state)
}

fn sample_session(tenant_id: Uuid, role: Role, username: &str) -> Session {
    Session {
        bearer_token: "tok".to_string(),
        tenant_id,
        username: username.to_string(),
        role,
        created_at: chrono::Utc::now(),
    }
}

async fn state_with_store(session_store: InMemorySessionStore) -> AppState {
    AppState {
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
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
}
}

async fn get_page(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    get_page_at(state, session_id, "/security/sessions").await
}

async fn get_page_at(state: AppState, session_id: &str, uri: &str) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn shows_an_empty_state_with_no_other_sessions_besides_the_caller() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "alice")).await;
    let state = state_with_store(store).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("alice"));
    assert!(body.contains("this session"));
}

#[tokio::test]
async fn marks_the_callers_own_revoke_button_with_an_accessible_label() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "alice")).await;
    let state = state_with_store(store).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains(r#"aria-label="Revoke -- use Log out instead of revoking your own session""#)
    );
}

#[tokio::test]
async fn sorts_by_username_ascending_when_sort_param_is_set() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "zoe")).await;
    store.create(sample_session(tenant_id, Role::Operator, "alice")).await;
    let state = state_with_store(store).await;

    let response = get_page_at(state, &session_id, "/security/sessions?sort=username").await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let alice_pos = body.find("alice").unwrap();
    let zoe_pos = body.find("zoe").unwrap();
    assert!(alice_pos < zoe_pos);
}

#[tokio::test]
async fn sorts_by_username_descending_when_dir_is_desc() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "zoe")).await;
    store.create(sample_session(tenant_id, Role::Operator, "alice")).await;
    let state = state_with_store(store).await;

    let response =
        get_page_at(state, &session_id, "/security/sessions?sort=username&dir=desc").await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let alice_pos = body.find("alice").unwrap();
    let zoe_pos = body.find("zoe").unwrap();
    assert!(zoe_pos < alice_pos);
}

#[tokio::test]
async fn defaults_to_most_recently_signed_in_first_when_sort_is_unset() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    // The caller's own session is created first here so it's the *older* one -- "bob" signs
    // in after, so "bob" should render first under the unset-sort default (newest first).
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "alice")).await;
    store.create(sample_session(tenant_id, Role::Operator, "bob")).await;
    let state = state_with_store(store).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let alice_pos = body.find("alice").unwrap();
    let bob_pos = body.find("bob").unwrap();
    assert!(bob_pos < alice_pos);
}

#[tokio::test]
async fn only_lists_sessions_for_the_callers_own_tenant() {
    let store = InMemorySessionStore::default();
    let tenant_a = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_a, Role::Admin, "alice")).await;
    store.create(sample_session(Uuid::new_v4(), Role::Admin, "bob-other-tenant")).await;
    let state = state_with_store(store).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("alice"));
    assert!(!body.contains("bob-other-tenant"));
}

#[tokio::test]
async fn filters_by_the_q_query_param_case_insensitively() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "alice")).await;
    store.create(sample_session(tenant_id, Role::Operator, "bob")).await;
    let state = state_with_store(store).await;

    let response = get_page_at(state, &session_id, "/security/sessions?q=ALICE").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("alice"));
    assert!(!body.contains("bob"));
}

#[tokio::test]
async fn shows_a_no_match_empty_state_for_an_unmatched_query() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let session_id = store.create(sample_session(tenant_id, Role::Admin, "alice")).await;
    let state = state_with_store(store).await;

    let response =
        get_page_at(state, &session_id, "/security/sessions?q=nobody-matches-this").await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No sessions match"));
}

#[tokio::test]
async fn non_admin_gets_forbidden() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Operator, "alice")).await;
    let state = state_with_store(store).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let state = state_with_store(InMemorySessionStore::default()).await;

    let response = router(state)
        .oneshot(Request::builder().uri("/security/sessions").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn revoke_removes_the_target_session() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let admin_session_id = store.create(sample_session(tenant_id, Role::Admin, "alice")).await;
    let target_session_id = store.create(sample_session(tenant_id, Role::Operator, "bob")).await;
    let state = state_with_store(store).await;

    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/security/sessions/{target_session_id}/revoke"))
                .header("cookie", format!("kizashi_session={admin_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(state.session_store.get(&target_session_id).await, None);
}

#[tokio::test]
async fn revoke_does_not_remove_a_session_belonging_to_a_different_tenant() {
    let store = InMemorySessionStore::default();
    let admin_session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let other_tenant_session_id =
        store.create(sample_session(Uuid::new_v4(), Role::Operator, "victim")).await;
    let state = state_with_store(store).await;

    router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/security/sessions/{other_tenant_session_id}/revoke"))
                .header("cookie", format!("kizashi_session={admin_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(state.session_store.get(&other_tenant_session_id).await.is_some());
}

#[tokio::test]
async fn revoke_requires_admin() {
    let store = InMemorySessionStore::default();
    let tenant_id = Uuid::new_v4();
    let operator_session_id =
        store.create(sample_session(tenant_id, Role::Operator, "alice")).await;
    let target_session_id = store.create(sample_session(tenant_id, Role::Operator, "bob")).await;
    let state = state_with_store(store).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/security/sessions/{target_session_id}/revoke"))
                .header("cookie", format!("kizashi_session={operator_session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
