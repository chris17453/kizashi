//! Tests for the `X-Username` header (CLAUDE.md §5: audit rows must record *who* acted, not
//! just *which tenant*). Split out from `user_handlers_test.rs` to keep both files under the
//! 500-line limit (CLAUDE.md §0.6); shares that file's `router`/`default_state`/`send_as` test
//! helpers rather than duplicating them.

use super::user_handlers_test::{default_state, router, send_as};
use super::*;
use crate::local_user_repository::local_user_repository_test::InMemoryLocalUserRepository;
use std::sync::Arc;

#[tokio::test]
async fn create_user_requires_a_username_header() {
    let tenant_id = Uuid::new_v4();
    let response = send_as(
        router(default_state()),
        "POST",
        "/v1/users".to_string(),
        Some(tenant_id),
        Some("admin"),
        None,
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_user_role_requires_a_username_header() {
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

    let response = send_as(
        router(state),
        "PUT",
        format!("/v1/users/{}", user.id),
        Some(tenant_id),
        Some("admin"),
        None,
        Some(serde_json::json!({"role": "admin"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_user_requires_a_username_header() {
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

    let response = send_as(
        router(state),
        "DELETE",
        format!("/v1/users/{}", user.id),
        Some(tenant_id),
        Some("admin"),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_user_threads_the_real_username_through_as_the_audit_actor() {
    let tenant_id = Uuid::new_v4();
    let repo = Arc::new(InMemoryLocalUserRepository::default());
    let state = AuthState { local_user_repository: repo.clone(), ..default_state() };

    let response = send_as(
        router(state),
        "POST",
        "/v1/users".to_string(),
        Some(tenant_id),
        Some("admin"),
        Some("carol-the-admin"),
        Some(serde_json::json!({"username": "bob", "password": "hunter2pass", "role": "operator"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(*repo.last_actor.lock().unwrap(), Some("carol-the-admin".to_string()));
}

#[tokio::test]
async fn update_user_role_threads_the_real_username_through_as_the_audit_actor() {
    let tenant_id = Uuid::new_v4();
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "bob".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Operator,
    };
    let repo = Arc::new(InMemoryLocalUserRepository::with_user(user.clone()));
    let state = AuthState { local_user_repository: repo.clone(), ..default_state() };

    let response = send_as(
        router(state),
        "PUT",
        format!("/v1/users/{}", user.id),
        Some(tenant_id),
        Some("admin"),
        Some("carol-the-admin"),
        Some(serde_json::json!({"role": "admin"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(*repo.last_actor.lock().unwrap(), Some("carol-the-admin".to_string()));
}

#[tokio::test]
async fn delete_user_threads_the_real_username_through_as_the_audit_actor() {
    let tenant_id = Uuid::new_v4();
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "bob".to_string(),
        password_hash: "hash".to_string(),
        role: Role::Operator,
    };
    let repo = Arc::new(InMemoryLocalUserRepository::with_user(user.clone()));
    let state = AuthState { local_user_repository: repo.clone(), ..default_state() };

    let response = send_as(
        router(state),
        "DELETE",
        format!("/v1/users/{}", user.id),
        Some(tenant_id),
        Some("admin"),
        Some("carol-the-admin"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(*repo.last_actor.lock().unwrap(), Some("carol-the-admin".to_string()));
}
