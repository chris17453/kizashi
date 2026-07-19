use super::*;
use crate::allowlist::allowlist_test::{FailingAllowlistRepository, InMemoryAllowlistRepository};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

fn state() -> AdminState {
    AdminState { allowlist_repository: Arc::new(InMemoryAllowlistRepository::default()) }
}

#[tokio::test]
async fn healthz_returns_200() {
    let app = build_router(state());
    let response =
        app.oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_allowlist_returns_empty_when_none_configured() {
    let app = build_router(state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", "tenant-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let domains: Vec<String> = serde_json::from_slice(&bytes).unwrap();
    assert!(domains.is_empty());
}

#[tokio::test]
async fn get_allowlist_requires_tenant_header() {
    let app = build_router(state());
    let response = app
        .oneshot(Request::builder().uri("/v1/allowlist").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_then_get_allowlist_round_trips() {
    let app_state = state();
    let body = serde_json::json!({"domains": ["zendesk.com"]});

    let put_response = build_router(app_state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/allowlist")
                .header("x-tenant-id", "tenant-a")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = build_router(app_state)
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", "tenant-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(get_response.into_body(), usize::MAX).await.unwrap();
    let domains: Vec<String> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(domains, vec!["zendesk.com".to_string()]);
}

#[tokio::test]
async fn get_allowlist_returns_500_on_backend_failure() {
    let app_state = AdminState { allowlist_repository: Arc::new(FailingAllowlistRepository) };
    let response = build_router(app_state)
        .oneshot(
            Request::builder()
                .uri("/v1/allowlist")
                .header("x-tenant-id", "tenant-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
