use super::*;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::audit_log::ChangeType;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use crate::login_attempt_repository::login_attempt_repository_test::InMemoryLoginAttemptRepository;
use crate::login_attempt_repository::LoginAttemptRepository;
use crate::mfa_repository::mfa_repository_test::InMemoryMfaChallengeRepository;
use crate::session_client::session_client_test::InMemorySessionClient;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use std::sync::Arc;
use tower::ServiceExt;

fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/users/:id/data-subject-export", get(get_data_subject_export))
        .with_state(state)
}

fn sample_user(tenant_id: Uuid, username: &str) -> LocalUser {
    LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: username.to_string(),
        password_hash: "hash".to_string(),
        role: Role::Operator,
        mfa_secret: None,
        mfa_enabled: false,
    }
}

fn state_with(
    users: InMemoryLocalUserRepository,
    audit: InMemoryAuditLogReader,
    attempts: InMemoryLoginAttemptRepository,
) -> AuthState {
    AuthState {
        local_user_repository: Arc::new(users),
        tenant_repository: Arc::new(
            crate::tenant_repository::tenant_repository_test::InMemoryTenantRepository::default(),
        ),
        tenant_branding_repository: Arc::new(
            crate::tenant_branding_repository::tenant_branding_repository_test::InMemoryTenantBrandingRepository::default(),
        ),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(audit),
        mfa_challenge_repository: Arc::new(InMemoryMfaChallengeRepository::default()),
        login_attempt_repository: Arc::new(attempts),
        session_audit_writer: Arc::new(
            crate::session_audit_writer::session_audit_writer_test::InMemorySessionAuditWriter::default(),
        ),
    }
}

async fn get_page(
    router: Router,
    id: Uuid,
    tenant_id: Option<Uuid>,
    role: Option<&str>,
) -> axum::http::Response<Body> {
    let mut req = Request::builder().uri(format!("/v1/users/{id}/data-subject-export"));
    if let Some(tenant_id) = tenant_id {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    if let Some(role) = role {
        req = req.header("x-role", role);
    }
    router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn admin_can_export_a_users_full_record() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "alice");
    let user_id = user.id;

    let audit = InMemoryAuditLogReader::default();
    audit.entries.lock().unwrap().push(AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "user".to_string(),
        entity_id: user_id,
        change_type: ChangeType::Created,
        actor: "bootstrap".to_string(),
        before: None,
        after: serde_json::json!({"username": "alice"}),
        changed_at: Utc::now(),
    });

    let attempts = InMemoryLoginAttemptRepository::default();
    attempts
        .record(&crate::login_attempt_repository::LoginAttempt {
            id: Uuid::new_v4(),
            tenant_id: Some(tenant_id),
            username: "alice".to_string(),
            success: false,
            reason: "wrong_password".to_string(),
            attempted_at: Utc::now(),
        })
        .await
        .unwrap();

    let state = state_with(InMemoryLocalUserRepository::with_user(user), audit, attempts);

    let response = get_page(router(state), user_id, Some(tenant_id), Some("admin")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["user"]["username"], "alice");
    assert!(body["user"].get("password_hash").is_none());
    assert_eq!(body["audit_log"].as_array().unwrap().len(), 1);
    assert_eq!(body["login_attempts"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn non_admin_is_forbidden() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "alice");
    let user_id = user.id;
    let state = state_with(
        InMemoryLocalUserRepository::with_user(user),
        InMemoryAuditLogReader::default(),
        InMemoryLoginAttemptRepository::default(),
    );

    let response = get_page(router(state), user_id, Some(tenant_id), Some("operator")).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_user_id_is_not_found() {
    let tenant_id = Uuid::new_v4();
    let state = state_with(
        InMemoryLocalUserRepository::default(),
        InMemoryAuditLogReader::default(),
        InMemoryLoginAttemptRepository::default(),
    );

    let response = get_page(router(state), Uuid::new_v4(), Some(tenant_id), Some("admin")).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_user_belonging_to_a_different_tenant_is_not_found() {
    let owning_tenant = Uuid::new_v4();
    let caller_tenant = Uuid::new_v4();
    let user = sample_user(owning_tenant, "victim");
    let user_id = user.id;
    let state = state_with(
        InMemoryLocalUserRepository::with_user(user),
        InMemoryAuditLogReader::default(),
        InMemoryLoginAttemptRepository::default(),
    );

    let response = get_page(router(state), user_id, Some(caller_tenant), Some("admin")).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
