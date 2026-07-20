use super::*;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository;
use crate::login_attempt_repository::{LoginAttempt, LoginAttemptRepository};
use crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository;
use crate::session_client::session_client_test::InMemorySessionClient;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AuthState) -> Router {
    Router::new().route("/v1/auth/local/login-attempts", get(get_login_attempts)).with_state(state)
}

fn state_with_repo(repo: Arc<InMemoryLoginAttemptRepository>) -> AuthState {
    AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(
            crate::tenant_repository::tenant_repository_test::InMemoryTenantRepository::default(),
        ),
        tenant_branding_repository: Arc::new(
            crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default(),
        ),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
        mfa_challenge_repository: Arc::new(InMemoryMfaChallengeRepository::default()),
        login_attempt_repository: repo,
        session_audit_writer: Arc::new(
            crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default(),
        ),
    }
}

async fn get_page(
    router: Router,
    tenant_id: Option<Uuid>,
    role: Option<&str>,
) -> axum::http::Response<Body> {
    let mut req = Request::builder().uri("/v1/auth/local/login-attempts");
    if let Some(tenant_id) = tenant_id {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    if let Some(role) = role {
        req = req.header("x-role", role);
    }
    router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn returns_recent_attempts_for_an_admin() {
    let tenant_id = Uuid::new_v4();
    let repo = Arc::new(InMemoryLoginAttemptRepository::default());
    repo.record(&LoginAttempt {
        id: Uuid::new_v4(),
        tenant_id: Some(tenant_id),
        username: "alice".to_string(),
        success: false,
        reason: "wrong_password".to_string(),
        attempted_at: Utc::now(),
    })
    .await
    .unwrap();
    let state = state_with_repo(repo);

    let response = get_page(router(state), Some(tenant_id), Some("admin")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["username"], "alice");
}

#[tokio::test]
async fn is_rejected_for_a_non_admin_role() {
    let tenant_id = Uuid::new_v4();
    let state = state_with_repo(Arc::new(InMemoryLoginAttemptRepository::default()));

    let response = get_page(router(state), Some(tenant_id), Some("operator")).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn requires_a_tenant_header() {
    let state = state_with_repo(Arc::new(InMemoryLoginAttemptRepository::default()));

    let response = get_page(router(state), None, Some("admin")).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn only_returns_the_callers_own_tenant() {
    let tenant_a = Uuid::new_v4();
    let repo = Arc::new(InMemoryLoginAttemptRepository::default());
    repo.record(&LoginAttempt {
        id: Uuid::new_v4(),
        tenant_id: Some(Uuid::new_v4()),
        username: "eve".to_string(),
        success: false,
        reason: "wrong_password".to_string(),
        attempted_at: Utc::now(),
    })
    .await
    .unwrap();
    let state = state_with_repo(repo);

    let response = get_page(router(state), Some(tenant_a), Some("admin")).await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}
