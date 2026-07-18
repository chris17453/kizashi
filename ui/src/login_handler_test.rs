use super::*;
use crate::auth_client::auth_client_test::{FailingAuthClient, InMemoryAuthClient};
use crate::events_client::events_client_test::InMemoryEventsClient;
use crate::health_client::health_client_test::InMemoryHealthClient;
use crate::health_client::PlatformHealthSummary;
use crate::session::InMemorySessionStore;
use crate::triggers_client::triggers_client_test::InMemoryTriggersClient;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn router(state: AppState) -> Router {
    Router::new().route("/login", get(get_login).post(post_login)).with_state(state)
}

fn default_state() -> AppState {
    AppState {
        session_store: Arc::new(InMemorySessionStore::default()),
        auth_client: Arc::new(InMemoryAuthClient::default()),
        events_client: Arc::new(InMemoryEventsClient::default()),
        triggers_client: Arc::new(InMemoryTriggersClient::default()),
        health_client: Arc::new(InMemoryHealthClient {
            summary: PlatformHealthSummary { status: "up".to_string(), services: vec![] },
        }),
        agents_client: Arc::new(crate::agents_client::agents_client_test::InMemoryAgentsClient::default()),
        stats_client: Arc::new(
            crate::ingestion_stats_client::ingestion_stats_client_test::InMemoryIngestionStatsClient::default(),
        ),
    }
}

#[tokio::test]
async fn get_login_renders_the_form() {
    let response = router(default_state())
        .oneshot(Request::builder().uri("/login").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Sign in"));
}

#[tokio::test]
async fn post_login_with_valid_credentials_sets_a_session_cookie_and_redirects() {
    let auth_client = InMemoryAuthClient::default();
    let tenant_id = Uuid::new_v4();
    *auth_client.result.lock().unwrap() = Some(("issued-token".to_string(), tenant_id));
    let state = AppState { auth_client: Arc::new(auth_client), ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("tenant_name=acme&username=alice&password=correct-password"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/events");
    let set_cookie = response.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("kizashi_session="));
    assert!(set_cookie.contains("HttpOnly"));
}

#[tokio::test]
async fn post_login_with_invalid_credentials_rerenders_the_form_with_an_error() {
    let state = AppState { auth_client: Arc::new(FailingAuthClient), ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("tenant_name=acme&username=alice&password=wrong"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Invalid workspace, username, or password"));
}

#[tokio::test]
async fn post_login_with_an_unknown_workspace_rerenders_the_form_with_an_error() {
    let state =
        AppState { auth_client: Arc::new(InMemoryAuthClient::default()), ..default_state() };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "tenant_name=nonexistent&username=alice&password=correct-password",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Invalid workspace, username, or password"));
}
