use super::*;
use crate::local_login_handler::AuthState;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use crate::local_user_repository::{LocalUser, LocalUserRepository};
use crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository;
use crate::password::{hash_password, verify_password};
use crate::session_client::session_client_test::InMemorySessionClient;
use axum::body::Body;
use axum::http::Request;
use axum::routing::post;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AuthState) -> Router {
    Router::new().route("/v1/auth/local/password", post(post_change_password)).with_state(state)
}

fn sample_user(tenant_id: Uuid, password: &str) -> LocalUser {
    LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "alice".to_string(),
        password_hash: hash_password(password).unwrap(),
        role: common::Role::Operator,
        mfa_secret: None,
        mfa_enabled: false,
    }
}

fn state_with_user(user: LocalUser) -> (AuthState, Arc<InMemoryLocalUserRepository>) {
    let repo = Arc::new(InMemoryLocalUserRepository::with_user(user));
    let state = AuthState {
        local_user_repository: repo.clone(),
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
        login_attempt_repository: Arc::new(
            crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default(),
        ),
    };
    (state, repo)
}

async fn post_json(
    router: Router,
    path: &str,
    headers: &[(&str, &str)],
    body: serde_json::Value,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method("POST").uri(path).header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    router.oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap()
}

#[tokio::test]
async fn changes_the_password_when_the_current_password_is_correct() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "old-correct-password");
    let (state, repo) = state_with_user(user.clone());

    let response = post_json(
        router(state),
        "/v1/auth/local/password",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({
            "current_password": "old-correct-password",
            "new_password": "a-brand-new-password-99"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(verify_password("a-brand-new-password-99", &updated.password_hash));
    assert!(!verify_password("old-correct-password", &updated.password_hash));
}

#[tokio::test]
async fn rejects_an_incorrect_current_password() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "old-correct-password");
    let (state, repo) = state_with_user(user.clone());

    let response = post_json(
        router(state),
        "/v1/auth/local/password",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({
            "current_password": "wrong-password",
            "new_password": "a-brand-new-password-99"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let unchanged = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(verify_password("old-correct-password", &unchanged.password_hash));
}

#[tokio::test]
async fn rejects_a_new_password_that_fails_the_policy() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "old-correct-password");
    let (state, _repo) = state_with_user(user.clone());

    let response = post_json(
        router(state),
        "/v1/auth/local/password",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({
            "current_password": "old-correct-password",
            "new_password": "short1"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn requires_a_username_header() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "old-correct-password");
    let (state, _repo) = state_with_user(user);

    let response = post_json(
        router(state),
        "/v1/auth/local/password",
        &[("x-tenant-id", &tenant_id.to_string())],
        serde_json::json!({
            "current_password": "old-correct-password",
            "new_password": "a-brand-new-password-99"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
