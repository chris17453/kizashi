use super::*;
use crate::local_user_repository::local_user_repository_test::{
    FailingLocalUserRepository, InMemoryLocalUserRepository,
};
use crate::local_user_repository::LocalUser;
use crate::password::hash_password;
use crate::session_client::session_client_test::{FailingSessionClient, InMemorySessionClient};
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
    }
}

#[tokio::test]
async fn correct_credentials_mint_a_session_token() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let session_client = Arc::new(InMemorySessionClient::default());
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        session_client: session_client.clone(),
        oidc_clients: std::collections::HashMap::new(),
    };

    let body = serde_json::json!({"tenant_id": tenant_id, "username": "alice", "password": "correct-password"});
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
    assert_eq!(session_client.minted.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn wrong_password_is_rejected_with_401() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id, "correct-password");
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::with_user(user)),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
    };

    let body = serde_json::json!({"tenant_id": tenant_id, "username": "alice", "password": "wrong-password"});
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
    let state = AuthState {
        local_user_repository: Arc::new(InMemoryLocalUserRepository::default()),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
    };

    let body = serde_json::json!({"tenant_id": Uuid::new_v4(), "username": "nobody", "password": "whatever"});
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
async fn repository_failure_returns_500() {
    let state = AuthState {
        local_user_repository: Arc::new(FailingLocalUserRepository),
        session_client: Arc::new(InMemorySessionClient::default()),
        oidc_clients: std::collections::HashMap::new(),
    };

    let body = serde_json::json!({"tenant_id": Uuid::new_v4(), "username": "alice", "password": "whatever"});
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
        session_client: Arc::new(FailingSessionClient),
        oidc_clients: std::collections::HashMap::new(),
    };

    let body = serde_json::json!({"tenant_id": tenant_id, "username": "alice", "password": "correct-password"});
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
