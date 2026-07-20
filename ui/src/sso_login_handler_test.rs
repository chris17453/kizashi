use super::*;
use crate::login_handler::login_handler_test::default_state;
use crate::oidc_client::oidc_client_test::{FailingOidcClient, InMemoryOidcClient};
use crate::oidc_client::{OidcAuthorization, OidcSession};
use crate::pending_oidc_flow::{
    InMemoryPendingOidcFlowStore, PendingOidcFlow, PendingOidcFlowStore,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use common::Role;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/login/sso", get(get_sso_login))
        .route("/login/sso/callback", get(get_sso_callback))
        .with_state(state)
}

#[tokio::test]
async fn get_sso_login_redirects_to_the_idp_and_sets_a_flow_cookie() {
    let oidc_client = Arc::new(InMemoryOidcClient::default());
    *oidc_client.authorize_result.lock().unwrap() = Some(OidcAuthorization {
        authorization_url: "https://idp.example.com/authorize?client_id=abc".to_string(),
        csrf_token: "csrf-123".to_string(),
        code_verifier: "verifier-123".to_string(),
    });
    let state = AppState { oidc_client, ..default_state() };

    let response = router(state)
        .oneshot(Request::builder().uri("/login/sso?tenant_name=acme").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "https://idp.example.com/authorize?client_id=abc"
    );
    let set_cookie = response.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("kizashi_oidc_flow="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
}

#[tokio::test]
async fn get_sso_login_shows_an_error_when_sso_is_not_configured() {
    let state = AppState { oidc_client: Arc::new(FailingOidcClient), ..default_state() };

    let response = router(state)
        .oneshot(Request::builder().uri("/login/sso?tenant_name=acme").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Single sign-on is not available"));
}

#[tokio::test]
async fn get_sso_callback_completes_the_flow_and_sets_a_session_cookie() {
    let flow_store = Arc::new(InMemoryPendingOidcFlowStore::default());
    let flow_id = flow_store
        .create(PendingOidcFlow {
            provider: "entra".to_string(),
            csrf_token: "csrf-123".to_string(),
            code_verifier: "verifier-123".to_string(),
            tenant_name: "acme".to_string(),
        })
        .await;

    let oidc_client = Arc::new(InMemoryOidcClient::default());
    let tenant_id = Uuid::new_v4();
    *oidc_client.callback_result.lock().unwrap() = Some(OidcSession {
        bearer_token: "issued-token".to_string(),
        tenant_id,
        role: Role::Viewer,
        username: Some("alice@example.com".to_string()),
    });

    let state = AppState { oidc_client, pending_oidc_flow_store: flow_store, ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/login/sso/callback?code=good-code&state=csrf-123")
                .header("cookie", format!("kizashi_oidc_flow={flow_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/overview");
    let set_cookie = response.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("kizashi_session="));
}

#[tokio::test]
async fn get_sso_callback_rejects_a_mismatched_state_csrf_token() {
    let flow_store = Arc::new(InMemoryPendingOidcFlowStore::default());
    let flow_id = flow_store
        .create(PendingOidcFlow {
            provider: "entra".to_string(),
            csrf_token: "csrf-123".to_string(),
            code_verifier: "verifier-123".to_string(),
            tenant_name: "acme".to_string(),
        })
        .await;

    let state = AppState { pending_oidc_flow_store: flow_store, ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/login/sso/callback?code=good-code&state=wrong-csrf")
                .header("cookie", format!("kizashi_oidc_flow={flow_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Sign-in request could not be verified"));
}

#[tokio::test]
async fn get_sso_callback_rejects_a_missing_flow_cookie() {
    let state = default_state();

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/login/sso/callback?code=good-code&state=csrf-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Sign-in request could not be verified"));
}

#[tokio::test]
async fn get_sso_callback_cannot_be_replayed() {
    let flow_store = Arc::new(InMemoryPendingOidcFlowStore::default());
    let flow_id = flow_store
        .create(PendingOidcFlow {
            provider: "entra".to_string(),
            csrf_token: "csrf-123".to_string(),
            code_verifier: "verifier-123".to_string(),
            tenant_name: "acme".to_string(),
        })
        .await;

    let oidc_client = Arc::new(InMemoryOidcClient::default());
    *oidc_client.callback_result.lock().unwrap() = Some(OidcSession {
        bearer_token: "issued-token".to_string(),
        tenant_id: Uuid::new_v4(),
        role: Role::Viewer,
        username: Some("alice@example.com".to_string()),
    });

    let state = AppState { oidc_client, pending_oidc_flow_store: flow_store, ..default_state() };

    let request = || {
        Request::builder()
            .uri("/login/sso/callback?code=good-code&state=csrf-123")
            .header("cookie", format!("kizashi_oidc_flow={flow_id}"))
            .body(Body::empty())
            .unwrap()
    };

    let first = router(state.clone()).oneshot(request()).await.unwrap();
    assert_eq!(first.status(), StatusCode::SEE_OTHER);

    let second = router(state).oneshot(request()).await.unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(second.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Sign-in request could not be verified"));
}
