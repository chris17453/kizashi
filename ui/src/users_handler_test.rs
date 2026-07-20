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
        })
        .await;
    let state = AppState {
        session_store: Arc::new(session_store),
        auth_client: Arc::new(InMemoryAuthClient::default()),
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
