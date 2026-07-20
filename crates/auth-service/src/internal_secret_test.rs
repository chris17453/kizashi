use crate::user_handlers::user_handlers_test::default_state;
use crate::{build_router, require_internal_secret};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SECRET: &str = "test-internal-secret";

fn test_router() -> Router {
    build_router(default_state(), TEST_SECRET.to_string())
}

#[tokio::test]
async fn protected_route_without_internal_secret_returns_401() {
    let app = test_router();

    let response = app
        .oneshot(Request::builder().uri("/v1/users").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_with_the_wrong_internal_secret_returns_401() {
    let app = test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/users")
                .header("x-internal-secret", "not-the-real-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_with_the_correct_internal_secret_reaches_the_handler() {
    let app = test_router();
    let tenant_id = Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/users")
                .header("x-internal-secret", TEST_SECRET)
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("x-username", "alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Reaches `list_users` (not blocked at the gate) and returns a normal, empty-tenant 200.
    assert_eq!(response.status(), StatusCode::OK);
}

/// Unit-level check of the middleware function itself, independent of `build_router`'s route
/// wiring — a plain two-route toy app so a future refactor of `build_router` can't silently
/// stop exercising this behavior.
fn toy_router() -> Router {
    async fn ok() -> &'static str {
        "ok"
    }
    Router::new()
        .route("/gated", get(ok))
        .layer(axum::middleware::from_fn_with_state(
            TEST_SECRET.to_string(),
            require_internal_secret,
        ))
        .with_state(())
}

#[tokio::test]
async fn require_internal_secret_rejects_a_missing_header() {
    let app = toy_router();

    let response =
        app.oneshot(Request::builder().uri("/gated").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn require_internal_secret_allows_the_correct_header() {
    let app = toy_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/gated")
                .header("x-internal-secret", TEST_SECRET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
