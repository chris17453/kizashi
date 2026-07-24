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
async fn get_users_shows_admin_only_nav_links_for_an_admin_session() {
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains("href=\"/users\""),
        "an Admin session should see the admin-only Users nav link"
    );
    assert!(
        body.contains("href=\"/security/sessions\""),
        "an Admin session should see the admin-only Active Sessions nav link"
    );
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
    assert!(body.contains(r#"aria-label="Role for alice""#));
    assert!(
        body.contains(r#"scope="col""#),
        "table headers should carry scope=\"col\" for screen readers"
    );
}

#[tokio::test]
async fn get_users_remove_button_asks_for_confirmation_before_submitting() {
    let (mut state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    let users_client = InMemoryUsersClient::default();
    users_client.users.lock().unwrap().push(UiUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "bob".to_string(),
        role: Role::Viewer,
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("onsubmit=\"return confirm("));
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
async fn get_users_filters_by_role() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "operator-user", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "viewer-user", "pw", Role::Viewer, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?role=operator")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("operator-user"));
    assert!(!text.contains("viewer-user"));
    assert!(text.contains("value=\"operator\" selected"));
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
async fn get_users_sort_header_links_percent_encode_a_q_containing_an_ampersand() {
    let (state, session_id, tenant_id) = state_with_session(Role::Admin).await;
    state
        .users_client
        .create_user(tenant_id, Role::Admin, "smith & co", "pw", Role::Operator, "test-actor")
        .await
        .unwrap();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/users?q=smith%20%26%20co")
                .header("cookie", cookie(&session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    // The raw search term contains "&", which -- if spliced unencoded into an href's query
    // string -- would be read as a new query parameter, corrupting the sort/dir that follow it.
    // Assert the actual link text, not just the page overall, since the input's own `value=`
    // attribute is expected to contain an HTML-escaped "&amp;" (a different, already-safe
    // escaping layer) and shouldn't be confused with the href bug this guards against.
    let link_start = text.find("/users?q=").expect("sort link missing");
    let link_end = text[link_start..].find('"').unwrap() + link_start;
    let link = &text[link_start..link_end];
    assert!(
        link.contains("smith%20%26%20co") || link.contains("smith+%26+co"),
        "q should be percent-encoded in the sort header href, got: {link}"
    );
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
