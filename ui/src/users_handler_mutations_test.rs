use super::*;
use crate::analysis_config_client::analysis_config_client_test::InMemoryAnalysisConfigClient;
use crate::audit_log_client::audit_log_client_test::InMemoryAuditLogClient;
use crate::auth_client::auth_client_test::InMemoryAuthClient;
use crate::egress_allowlist_client::egress_allowlist_client_test::InMemoryEgressAllowlistClient;
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
use crate::users_client::users_client_test::InMemoryUsersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/users", get(get_users).post(post_users))
        .route("/users/:id/role", post(post_update_user_role))
        .route("/users/:id/delete", post(post_delete_user))
        .route("/users/bulk-delete", post(post_bulk_delete_users))
        .route("/users/bulk-role", post(post_bulk_update_user_role))
        .route("/users/:id/export", get(get_export_user))
        .with_state(state)
}

async fn state_with_session(role: Role) -> (AppState, String, Uuid) {
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
        incidents_client: Arc::new(crate::incidents_client::incidents_client_test::InMemoryIncidentsClient::default()),
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
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(crate::saved_search_queries_client::saved_search_queries_client_test::InMemorySavedSearchQueriesClient::default()),
        config_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        retention_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        auth_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        ingestion_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        egress_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
            mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
            login_attempts_client: Arc::new(crate::login_attempts_client::login_attempts_client_test::InMemoryLoginAttemptsClient::default()),
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
};
    (state, session_id, tenant_id)
}

fn cookie(session_id: &str) -> String {
    format!("{}={}", crate::SESSION_COOKIE_NAME, session_id)
}

#[tokio::test]
async fn post_users_creates_a_user_and_redirects() {
    let (state, session_id, _) = state_with_session(Role::Admin).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=bob&password=a-real-password&role=operator"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn post_users_is_forbidden_for_an_operator() {
    let (state, session_id, _) = state_with_session(Role::Operator).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=bob&password=a-real-password&role=operator"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_update_user_role_changes_the_role_and_redirects() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let created = state
        .users_client
        .create_user(tenant_id, Role::Admin, "bob", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/role", created.id))
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("role=admin"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn post_delete_user_removes_the_user_and_redirects() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let created = state
        .users_client
        .create_user(tenant_id, Role::Admin, "bob", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", created.id))
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn post_delete_user_is_forbidden_for_an_operator() {
    let (state, session_id, _) = state_with_session(Role::Operator).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/users/{}/delete", Uuid::new_v4()))
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_bulk_delete_users_removes_every_selected_user() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let first = state
        .users_client
        .create_user(tenant_id, Role::Admin, "first", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();
    let second = state
        .users_client
        .create_user(tenant_id, Role::Admin, "second", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "untouched", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/bulk-delete")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={}&ids={}", first.id, second.id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let remaining = state.users_client.list_users(tenant_id, Role::Admin).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].username, "untouched");
}

#[tokio::test]
async fn post_bulk_delete_users_with_no_selection_is_a_no_op() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "untouched", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/bulk-delete")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let remaining = state.users_client.list_users(tenant_id, Role::Admin).await.unwrap();
    assert_eq!(remaining.len(), 1);
}

#[tokio::test]
async fn post_bulk_update_user_role_changes_every_selected_user() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let first = state
        .users_client
        .create_user(tenant_id, Role::Admin, "first", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();
    let second = state
        .users_client
        .create_user(tenant_id, Role::Admin, "second", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();

    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/bulk-role")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={}&ids={}&role=operator", first.id, second.id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let users = state.users_client.list_users(tenant_id, Role::Admin).await.unwrap();
    assert!(users
        .iter()
        .filter(|user| user.username != "alice")
        .all(|user| user.role == Role::Operator));
}

#[tokio::test]
async fn post_bulk_update_user_role_is_forbidden_for_an_operator() {
    let (state, session_id, tenant_id) = state_with_session(Role::Operator).await;
    let created = state
        .users_client
        .create_user(tenant_id, Role::Admin, "first", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/bulk-role")
                .header("cookie", cookie(&session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={}&role=admin", created.id)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_bulk_delete_users_is_forbidden_for_an_operator() {
    let (state, _session_id, tenant_id) = state_with_session(Role::Admin).await;
    let created = state
        .users_client
        .create_user(tenant_id, Role::Admin, "first", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let session_store = InMemorySessionStore::default();
    let operator_session_id = session_store
        .create(Session {
            bearer_token: "tok".to_string(),
            tenant_id,
            username: "operator-user".to_string(),
            role: Role::Operator,
            created_at: chrono::Utc::now(),
        })
        .await;
    let mut operator_state = state.clone();
    operator_state.session_store = Arc::new(session_store);

    let response = router(operator_state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users/bulk-delete")
                .header("cookie", cookie(&operator_session_id))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("ids={}", created.id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let remaining = state.users_client.list_users(tenant_id, Role::Admin).await.unwrap();
    assert_eq!(remaining.len(), 1);
}
