use super::*;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post, put};
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;

pub(crate) fn router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/users", post(create_user).get(list_users))
        .route("/v1/users/:id", put(update_user_role).delete(delete_user))
        .route("/v1/users/:id/audit-log", get(get_user_audit_log))
        .with_state(state)
}

pub(crate) fn default_state() -> AuthState {
    AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        tenant_repository: Arc::new(
            crate::tenant_repository::tenant_repository_test::InMemoryTenantRepository::default(),
        ),
        session_client: Arc::new(
            crate::session_client::session_client_test::InMemorySessionClient::default(),
        ),
        oidc_clients: std::collections::HashMap::new(),
        audit_log_reader: Arc::new(InMemoryAuditLogReader::default()),
    }
}

async fn send(
    app: Router,
    method: &str,
    uri: String,
    tenant_id: Option<Uuid>,
    role: Option<&str>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    send_as(app, method, uri, tenant_id, role, Some("test-actor"), body).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn send_as(
    app: Router,
    method: &str,
    uri: String,
    tenant_id: Option<Uuid>,
    role: Option<&str>,
    username: Option<&str>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(tenant_id) = tenant_id {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    if let Some(role) = role {
        req = req.header("x-role", role);
    }
    if let Some(username) = username {
        req = req.header("x-username", username);
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_user_succeeds_for_an_admin() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "POST",
        "/v1/users".to_string(),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_user_is_rejected_for_an_operator() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "POST",
        "/v1/users".to_string(),
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_user_is_rejected_for_a_viewer() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "POST",
        "/v1/users".to_string(),
        Some(tenant_id),
        Some("viewer"),
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_user_requires_a_tenant_header() {
    let response = send(
        router(default_state()),
        "POST",
        "/v1/users".to_string(),
        None,
        Some("admin"),
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_user_never_returns_the_password_hash() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "POST",
        "/v1/users".to_string(),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("password_hash").is_none());
    assert!(json.get("password").is_none());
}

#[tokio::test]
async fn list_users_returns_only_the_callers_tenant() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    state
        .local_user_repository
        .create(
            LocalUser {
                id: Uuid::new_v4(),
                tenant_id,
                username: "alice".to_string(),
                password_hash: "hash".to_string(),
                role: Role::Admin,
            },
            "test-actor",
        )
        .await
        .unwrap();
    state
        .local_user_repository
        .create(
            LocalUser {
                id: Uuid::new_v4(),
                tenant_id: Uuid::new_v4(),
                username: "eve".to_string(),
                password_hash: "hash".to_string(),
                role: Role::Admin,
            },
            "test-actor",
        )
        .await
        .unwrap();

    let response =
        send(router(state), "GET", "/v1/users".to_string(), Some(tenant_id), Some("admin"), None)
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["username"], "alice");
}

#[tokio::test]
async fn update_user_role_changes_the_role() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "bob".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Operator,
    };
    state.local_user_repository.create(user.clone(), "test-actor").await.unwrap();

    let response = send(
        router(state),
        "PUT",
        format!("/v1/users/{}", user.id),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"role": "admin"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["role"], "admin");
}

#[tokio::test]
async fn update_user_role_for_an_unknown_id_returns_404() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/users/{}", Uuid::new_v4()),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"role": "admin"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_user_removes_the_user() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "bob".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Operator,
    };
    state.local_user_repository.create(user.clone(), "test-actor").await.unwrap();

    let response = send(
        router(state),
        "DELETE",
        format!("/v1/users/{}", user.id),
        Some(tenant_id),
        Some("admin"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_user_rejects_deleting_the_last_admin() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let admin = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "sole-admin".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    state.local_user_repository.create(admin.clone(), "test-actor").await.unwrap();

    let response = send(
        router(state),
        "DELETE",
        format!("/v1/users/{}", admin.id),
        Some(tenant_id),
        Some("admin"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn delete_user_allows_deleting_an_admin_when_another_admin_remains() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let admin_one = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "admin-one".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    let admin_two = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "admin-two".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    state.local_user_repository.create(admin_one.clone(), "test-actor").await.unwrap();
    state.local_user_repository.create(admin_two, "test-actor").await.unwrap();

    let response = send(
        router(state),
        "DELETE",
        format!("/v1/users/{}", admin_one.id),
        Some(tenant_id),
        Some("admin"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn update_user_role_rejects_demoting_the_last_admin() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let admin = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "sole-admin".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    state.local_user_repository.create(admin.clone(), "test-actor").await.unwrap();

    let response = send(
        router(state),
        "PUT",
        format!("/v1/users/{}", admin.id),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn update_user_role_allows_reassigning_the_sole_admin_to_admin() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let admin = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "sole-admin".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    state.local_user_repository.create(admin.clone(), "test-actor").await.unwrap();

    let response = send(
        router(state),
        "PUT",
        format!("/v1/users/{}", admin.id),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"role": "admin"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn update_user_role_allows_demoting_an_admin_when_another_admin_remains() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let admin_one = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "admin-one".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    let admin_two = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "admin-two".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Admin,
    };
    state.local_user_repository.create(admin_one.clone(), "test-actor").await.unwrap();
    state.local_user_repository.create(admin_two, "test-actor").await.unwrap();

    let response = send(
        router(state),
        "PUT",
        format!("/v1/users/{}", admin_one.id),
        Some(tenant_id),
        Some("admin"),
        Some(serde_json::json!({"role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_user_is_rejected_for_an_operator() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "DELETE",
        format!("/v1/users/{}", Uuid::new_v4()),
        Some(tenant_id),
        Some("operator"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_user_audit_log_returns_entries_for_the_tenant() {
    let tenant_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    let audit_log_reader = Arc::new(InMemoryAuditLogReader::default());
    audit_log_reader.entries.lock().unwrap().push(crate::audit_log::AuditLogEntry {
        id: Uuid::new_v4(),
        tenant_id,
        entity_type: "local_user".to_string(),
        entity_id,
        change_type: crate::audit_log::ChangeType::Created,
        actor: tenant_id.to_string(),
        before: None,
        after: serde_json::json!({"username": "bob"}),
        changed_at: chrono::Utc::now(),
    });
    let state = AuthState { audit_log_reader, ..default_state() };

    let response = send(
        router(state),
        "GET",
        format!("/v1/users/{entity_id}/audit-log"),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_user_audit_log_returns_500_on_backend_failure() {
    let tenant_id = Uuid::new_v4();
    let state = AuthState {
        audit_log_reader: Arc::new(crate::audit_log::audit_log_test::FailingAuditLogReader),
        ..default_state()
    };

    let response = send(
        router(state),
        "GET",
        format!("/v1/users/{}/audit-log", Uuid::new_v4()),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
