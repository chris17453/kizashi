use super::*;
use crate::local_user_repository::local_user_repository_test::{
    FailingLocalUserRepository, InMemoryLocalUserRepository,
};
use crate::local_user_repository::LocalUser;
use crate::password::hash_password;
use crate::session_client::session_client_test::{FailingSessionClient, InMemorySessionClient};
use crate::tenant_repository::tenant_repository_test::{
    FailingTenantRepository, InMemoryTenantRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::post;
use axum::Router;
use tower::ServiceExt;

fn router(state: AuthState) -> Router {
    Router::new().route("/v1/auth/local/login", post(local_login)).with_state(state)
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

#[tokio::test]
async fn correct_credentials_mint_a_session_token() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let session_client = Arc::new(InMemorySessionClient::default());
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: session_client.clone(),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "correct-password"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    {
        let minted = session_client.minted.lock().unwrap();
        assert_eq!(minted.len(), 1);
        assert_eq!(minted[0].1, common::Role::Operator);
    }

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["role"], "operator");
}

#[tokio::test]
async fn wrong_password_is_rejected_with_401() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "wrong-password"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_username_is_rejected_with_401_not_404() {
    let tenant_id = Uuid::new_v4();
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body =
        serde_json::json!({"tenant_name": "acme", "username": "nobody", "password": "whatever"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "must not leak whether the username exists via status code"
    );
}

#[tokio::test]
async fn unknown_tenant_name_is_rejected_with_401_not_404() {
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(InMemoryTenantRepository::default()),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body = serde_json::json!({"tenant_name": "nonexistent", "username": "alice", "password": "whatever"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "must not leak whether the workspace exists via status code"
    );
}

#[tokio::test]
async fn repository_failure_returns_500() {
    let tenant_id = Uuid::new_v4();
    let state = AuthState {
        local_user_repository: Arc::new(FailingLocalUserRepository),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body =
        serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "whatever"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn tenant_repository_failure_returns_500() {
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(FailingTenantRepository),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body =
        serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "whatever"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn session_mint_failure_returns_502() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(FailingSessionClient),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
            mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "correct-password"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn correct_credentials_for_an_mfa_enabled_user_returns_a_challenge_not_a_session() {
    let tenant_id = Uuid::new_v4();
    let mut user = sample_user(tenant_id, "correct-password");
    user.mfa_enabled = true;
    let session_client = Arc::new(InMemorySessionClient::default());
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: session_client.clone(),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
        mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
            login_attempt_repository: Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default()),
            session_audit_writer: Arc::new(crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default()),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "correct-password"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(session_client.minted.lock().unwrap().len(), 0, "no session should be minted yet");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["mfa_required"], true);
    assert!(json["challenge_token"].as_str().is_some_and(|t| !t.is_empty()));
}

#[tokio::test]
async fn a_successful_login_records_a_successful_attempt() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let login_attempt_repository =
        Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default());
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
        mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
        login_attempt_repository: login_attempt_repository.clone(),
        session_audit_writer: Arc::new(
            crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default(),
        ),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "correct-password"});
    router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let attempts = login_attempt_repository.attempts.lock().unwrap();
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].success);
    assert_eq!(attempts[0].reason, "success");
    assert_eq!(attempts[0].tenant_id, Some(tenant_id));
}

#[tokio::test]
async fn a_wrong_password_records_a_failed_attempt() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let login_attempt_repository =
        Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default());
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
        mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
        login_attempt_repository: login_attempt_repository.clone(),
        session_audit_writer: Arc::new(
            crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default(),
        ),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "wrong-password"});
    router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let attempts = login_attempt_repository.attempts.lock().unwrap();
    assert_eq!(attempts.len(), 1);
    assert!(!attempts[0].success);
    assert_eq!(attempts[0].reason, "wrong_password");
}

#[tokio::test]
async fn an_unknown_workspace_records_a_failed_attempt_with_no_tenant_id() {
    let login_attempt_repository =
        Arc::new(crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository::default());
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(InMemoryTenantRepository::default()),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
        mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
        login_attempt_repository: login_attempt_repository.clone(),
        session_audit_writer: Arc::new(
            crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default(),
        ),
    };

    let body = serde_json::json!({"tenant_name": "nonexistent", "username": "alice", "password": "whatever"});
    router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let attempts = login_attempt_repository.attempts.lock().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].tenant_id, None);
    assert_eq!(attempts[0].reason, "unknown_workspace");
}

#[tokio::test]
async fn a_login_still_succeeds_even_if_recording_the_attempt_fails() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        tenant_repository: Arc::new(InMemoryTenantRepository::with_tenant("acme", tenant_id)),
        tenant_branding_repository: Arc::new(crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(
            crate::audit_log::audit_log_test::InMemoryAuditLogReader::default(),
        ),
        mfa_challenge_repository: Arc::new(crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository::default()),
        login_attempt_repository: Arc::new(
            crate::login_attempt_repository::login_attempt_repository_test::FailingLoginAttemptRepository,
        ),
        session_audit_writer: Arc::new(
            crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default(),
        ),
    };

    let body = serde_json::json!({"tenant_name": "acme", "username": "alice", "password": "correct-password"});
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a failure to record the attempt must not break the login response itself"
    );
}
