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
use crate::login_attempts_client::login_attempts_client_test::{
    FailingLoginAttemptsClient, InMemoryLoginAttemptsClient,
};
use crate::login_attempts_client::{LoginAttempt, LoginAttemptsClient};
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
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use std::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/security/login-attempts", get(get_login_attempts))
        .route("/security/login-attempts/export.csv", get(get_login_attempts_export_csv))
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

async fn state_with(
    session_store: InMemorySessionStore,
    login_attempts_client: Arc<dyn LoginAttemptsClient>,
) -> AppState {
    AppState {
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
        egress_audit_log_client: Arc::new(InMemoryAuditLogClient::default()),
        users_client: Arc::new(InMemoryUsersClient::default()),
        saved_search_queries_client: Arc::new(InMemorySavedSearchQueriesClient::default()),
        ingestion_gateway_public_url: "http://localhost:8081".to_string(),
        mfa_client: Arc::new(crate::mfa_client::mfa_client_test::InMemoryMfaClient::default()),
        login_attempts_client,
                backup_status_client: Arc::new(crate::backup_status_client::backup_status_client_test::InMemoryBackupStatusClient::default()),
}
}

async fn get_page(state: AppState, session_id: &str) -> axum::http::Response<Body> {
    get_page_at(state, session_id, "/security/login-attempts").await
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

fn attempt(username: &str, success: bool) -> LoginAttempt {
    LoginAttempt {
        username: username.to_string(),
        success,
        reason: "wrong_password".to_string(),
        attempted_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn admin_can_view_login_attempts() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let client = InMemoryLoginAttemptsClient {
        attempts: Mutex::new(vec![LoginAttempt {
            username: "bob".to_string(),
            success: false,
            reason: "wrong_password".to_string(),
            attempted_at: chrono::Utc::now(),
        }]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("bob"));
    assert!(body.contains("wrong_password"));
    assert!(body.contains("1 failed attempt"));
}

#[tokio::test]
async fn shows_an_empty_state_with_no_attempts() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let state = state_with(store, Arc::new(InMemoryLoginAttemptsClient::default())).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("No recent login attempts"));
}

#[tokio::test]
async fn shows_an_error_when_the_backend_is_unreachable() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let state = state_with(store, Arc::new(FailingLoginAttemptsClient)).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("simulated failure"));
}

#[tokio::test]
async fn non_admin_gets_forbidden() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Operator, "alice")).await;
    let state = state_with(store, Arc::new(InMemoryLoginAttemptsClient::default())).await;

    let response = get_page(state, &session_id).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn redirects_to_login_when_not_signed_in() {
    let state = state_with(
        InMemorySessionStore::default(),
        Arc::new(InMemoryLoginAttemptsClient::default()),
    )
    .await;

    let response = router(state)
        .oneshot(Request::builder().uri("/security/login-attempts").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn filters_by_the_q_query_param_case_insensitively() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let client = InMemoryLoginAttemptsClient {
        attempts: Mutex::new(vec![attempt("bob", false), attempt("carol", false)]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page_at(state, &session_id, "/security/login-attempts?q=BOB").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("bob"));
    assert!(!body.contains("carol"));
}

#[tokio::test]
async fn filters_by_outcome_and_preserves_investigation_scope() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let client = InMemoryLoginAttemptsClient {
        attempts: Mutex::new(vec![attempt("bob", false), attempt("carol", true)]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page_at(state, &session_id, "/security/login-attempts?status=failed").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("bob"));
    assert!(!body.contains("carol"));
    assert!(body.contains("name=\"status\""));
    assert!(body.contains("value=\"failed\" selected"));
}

#[tokio::test]
async fn shows_a_load_older_link_when_a_full_page_is_returned() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let full_page: Vec<LoginAttempt> =
        (0..DEFAULT_LIMIT).map(|i| attempt(&format!("user-{i}"), false)).collect();
    let client = InMemoryLoginAttemptsClient { attempts: Mutex::new(full_page) };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Load older"));
}

#[tokio::test]
async fn does_not_show_a_load_older_link_for_a_partial_page() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let client = InMemoryLoginAttemptsClient { attempts: Mutex::new(vec![attempt("bob", false)]) };
    let state = state_with(store, Arc::new(client)).await;

    let response = get_page(state, &session_id).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("Load older"));
}

#[tokio::test]
async fn honors_the_before_query_param_as_the_starting_cursor() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let mut newer = attempt("newer-user", false);
    newer.attempted_at = "2026-07-19T00:00:00Z".parse().unwrap();
    let mut older = attempt("older-user", false);
    older.attempted_at = "2026-07-17T00:00:00Z".parse().unwrap();
    let client = InMemoryLoginAttemptsClient { attempts: Mutex::new(vec![newer, older]) };
    let state = state_with(store, Arc::new(client)).await;

    let response =
        get_page_at(state, &session_id, "/security/login-attempts?before=2026-07-18T00:00:00Z")
            .await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("older-user"));
    assert!(!body.contains("newer-user"));
}

#[tokio::test]
async fn export_csv_returns_every_attempt_as_csv() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let client = InMemoryLoginAttemptsClient {
        attempts: Mutex::new(vec![attempt("bob", false), attempt("carol", true)]),
    };
    let state = state_with(store, Arc::new(client)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/login-attempts/export.csv")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap().to_str().unwrap(), "text/csv");
    let disposition = response.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(disposition.contains("login-attempts-"));
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.starts_with("attempted_at,username,success,reason\n"));
    assert!(body.contains("bob"));
    assert!(body.contains("carol"));
}

#[tokio::test]
async fn export_csv_requires_admin_role() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Operator, "alice")).await;
    let state = state_with(store, Arc::new(InMemoryLoginAttemptsClient::default())).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/login-attempts/export.csv")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_attempt_reason_filter_and_posture_handoff_are_available() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session(Uuid::new_v4(), Role::Admin, "alice")).await;
    let client = InMemoryLoginAttemptsClient {
        attempts: Mutex::new(vec![attempt("bob", false), attempt("carol", true)]),
    };
    let state = state_with(store, Arc::new(client)).await;
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/security/login-attempts?reason=wrong_password")
                .header("cookie", format!("kizashi_session={session_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Failure reason"));
    assert!(body.contains("name=\"reason\""));
}
