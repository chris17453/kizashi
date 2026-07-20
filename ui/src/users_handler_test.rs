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
use crate::users_client::users_client_test::{FailingUsersClient, InMemoryUsersClient};
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
async fn get_users_succeeds_for_an_admin() {
    let (state, session_id, _) = state_with_session(Role::Admin).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_users_marks_the_current_users_remove_button_with_an_accessible_label() {
    let (mut state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let users_client = InMemoryUsersClient::default();
    users_client.users.lock().unwrap().push(UiUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "alice".to_string(),
        role: Role::Admin,
        mfa_enabled: false,
    });
    state.users_client = Arc::new(users_client);
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains(r#"aria-label="Remove -- you can't remove yourself""#));
}

#[tokio::test]
async fn get_users_is_forbidden_for_an_operator() {
    let (state, session_id, _) = state_with_session(Role::Operator).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_users_is_forbidden_for_a_viewer() {
    let (state, session_id, _) = state_with_session(Role::Viewer).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_users_redirects_without_a_session() {
    let (state, _, _) = state_with_session(Role::Admin).await;
    let response = router(state)
        .oneshot(Request::builder().uri("/users").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_users_shows_backend_error() {
    let (mut state, session_id, _) = state_with_session(Role::Admin).await;
    state.users_client = Arc::new(FailingUsersClient);
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("simulated failure"));
}

#[tokio::test]
async fn get_users_filters_by_the_q_query_param_case_insensitively() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "alice-ops", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "bob-viewer", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?q=ALICE")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("alice-ops"));
    assert!(!text.contains("bob-viewer"));
}

#[tokio::test]
async fn get_users_shows_a_no_match_empty_state_for_an_unmatched_query() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "alice-ops", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?q=nobody-matches-this")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("No users match"));
}

#[tokio::test]
async fn get_users_sorts_by_username_ascending_by_default_when_sort_param_is_set() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "zed-user", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "aaa-user", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?sort=username")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let aaa_pos = text.find("aaa-user").expect("aaa-user missing");
    let zed_pos = text.find("zed-user").expect("zed-user missing");
    assert!(aaa_pos < zed_pos, "expected ascending username order");
}

#[tokio::test]
async fn get_users_sorts_descending_when_dir_is_desc() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "zed-user", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "aaa-user", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?sort=username&dir=desc")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let aaa_pos = text.find("aaa-user").expect("aaa-user missing");
    let zed_pos = text.find("zed-user").expect("zed-user missing");
    assert!(zed_pos < aaa_pos, "expected descending username order");
}

#[tokio::test]
async fn get_users_sorts_by_role() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(
            tenant_id,
            Role::Admin,
            "user-with-viewer-role",
            "pw",
            Role::Viewer,
            "test-actor",
        )
        .await
        .unwrap();
    state
        .users_client
        .create_user(
            tenant_id,
            Role::Admin,
            "user-with-admin-role",
            "pw",
            Role::Admin,
            "test-actor",
        )
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?sort=role")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    // "admin" < "viewer" alphabetically, so ascending role sort puts the admin row first.
    let admin_pos = text.find("user-with-admin-role").expect("admin row missing");
    let viewer_pos = text.find("user-with-viewer-role").expect("viewer row missing");
    assert!(admin_pos < viewer_pos, "expected alphabetical role order");
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
async fn get_export_user_downloads_a_json_attachment_for_an_admin() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let created = state
        .users_client
        .create_user(tenant_id, Role::Admin, "bob", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/users/{}/export", created.id))
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-disposition").unwrap(),
        &format!("attachment; filename=\"user-{}-export.json\"", created.id)
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user"]["username"], "bob");
}

#[tokio::test]
async fn get_export_user_is_forbidden_for_an_operator() {
    let (state, session_id, _) = state_with_session(Role::Operator).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/users/{}/export", Uuid::new_v4()))
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
