use super::*;
use crate::local_login_handler::AuthState;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use crate::local_user_repository::{LocalUser, LocalUserRepository};
use crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository;
use crate::password::hash_password;
use crate::session_client::session_client_test::InMemorySessionClient;
use axum::body::Body;
use axum::http::Request;
use axum::routing::post;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/auth/local/mfa/status", axum::routing::get(get_mfa_status))
        .route("/v1/auth/local/mfa/enroll", post(post_mfa_enroll))
        .route("/v1/auth/local/mfa/verify", post(post_mfa_verify))
        .route("/v1/auth/local/mfa/disable", post(post_mfa_disable))
        .route("/v1/auth/local/mfa/challenge", post(post_mfa_challenge))
        .with_state(state)
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
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
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
async fn enroll_stores_a_pending_secret_and_returns_a_provisioning_uri() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/enroll",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(body["provisioning_uri"].as_str().unwrap().starts_with("otpauth://"));

    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(found.mfa_secret.is_some());
    assert!(!found.mfa_enabled, "enrollment alone must not enable MFA");
}

#[tokio::test]
async fn verify_with_the_correct_code_enables_mfa() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    let enrollment = crate::mfa::generate_enrollment("alice").unwrap();
    repo.set_pending_mfa_secret(user.id, &enrollment.secret_base32).await.unwrap();

    let code = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(enrollment.secret_base32.clone()).to_bytes().unwrap(),
        None,
        "alice".to_string(),
    )
    .unwrap()
    .generate_current()
    .unwrap();

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/verify",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({"code": code}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(found.mfa_enabled);
}

#[tokio::test]
async fn verify_with_the_wrong_code_is_rejected_and_does_not_enable_mfa() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    let enrollment = crate::mfa::generate_enrollment("alice").unwrap();
    repo.set_pending_mfa_secret(user.id, &enrollment.secret_base32).await.unwrap();

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/verify",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({"code": "000000"}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(!found.mfa_enabled);
}

#[tokio::test]
async fn verify_without_a_pending_enrollment_is_rejected() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, _repo) = state_with_user(user);

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/verify",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({"code": "123456"}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn disable_with_the_correct_password_clears_mfa() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    repo.set_pending_mfa_secret(user.id, "SOMESECRET").await.unwrap();
    repo.confirm_mfa(user.id).await.unwrap();

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/disable",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({"password": "correct-password"}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(!found.mfa_enabled);
    assert_eq!(found.mfa_secret, None);
}

#[tokio::test]
async fn disable_with_the_wrong_password_is_rejected() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    repo.set_pending_mfa_secret(user.id, "SOMESECRET").await.unwrap();
    repo.confirm_mfa(user.id).await.unwrap();

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/disable",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
        serde_json::json!({"password": "wrong-password"}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(found.mfa_enabled, "wrong password must not disable MFA");
}

#[tokio::test]
async fn challenge_with_the_correct_code_mints_a_session() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    let enrollment = crate::mfa::generate_enrollment("alice").unwrap();
    repo.set_pending_mfa_secret(user.id, &enrollment.secret_base32).await.unwrap();
    repo.confirm_mfa(user.id).await.unwrap();
    let challenge_token = state.mfa_challenge_repository.create(user.id, tenant_id).await.unwrap();
    let code = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(enrollment.secret_base32.clone()).to_bytes().unwrap(),
        None,
        "alice".to_string(),
    )
    .unwrap()
    .generate_current()
    .unwrap();

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/challenge",
        &[],
        serde_json::json!({"challenge_token": challenge_token, "code": code}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["tenant_id"], tenant_id.to_string());
}

#[tokio::test]
async fn challenge_token_is_single_use() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    let enrollment = crate::mfa::generate_enrollment("alice").unwrap();
    repo.set_pending_mfa_secret(user.id, &enrollment.secret_base32).await.unwrap();
    repo.confirm_mfa(user.id).await.unwrap();
    let challenge_token = state.mfa_challenge_repository.create(user.id, tenant_id).await.unwrap();
    let code = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(enrollment.secret_base32.clone()).to_bytes().unwrap(),
        None,
        "alice".to_string(),
    )
    .unwrap()
    .generate_current()
    .unwrap();

    let app = router(state);
    let first = post_json(
        app.clone(),
        "/v1/auth/local/mfa/challenge",
        &[],
        serde_json::json!({"challenge_token": challenge_token, "code": code}),
    )
    .await;
    let second = post_json(
        app,
        "/v1/auth/local/mfa/challenge",
        &[],
        serde_json::json!({"challenge_token": challenge_token, "code": code}),
    )
    .await;

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn challenge_with_an_unknown_token_is_rejected() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, _repo) = state_with_user(user);

    let response = post_json(
        router(state),
        "/v1/auth/local/mfa/challenge",
        &[],
        serde_json::json!({"challenge_token": "bogus", "code": "123456"}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

async fn get(router: Router, path: &str, headers: &[(&str, &str)]) -> axum::http::Response<Body> {
    let mut req = Request::builder().uri(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn status_reports_disabled_for_a_fresh_user() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, _repo) = state_with_user(user);

    let response = get(
        router(state),
        "/v1/auth/local/mfa/status",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["enabled"], false);
}

#[tokio::test]
async fn status_reports_enabled_after_confirmation() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let (state, repo) = state_with_user(user.clone());
    repo.set_pending_mfa_secret(user.id, "SOMESECRET").await.unwrap();
    repo.confirm_mfa(user.id).await.unwrap();

    let response = get(
        router(state),
        "/v1/auth/local/mfa/status",
        &[("x-tenant-id", &tenant_id.to_string()), ("x-username", "alice")],
    )
    .await;

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["enabled"], true);
}
