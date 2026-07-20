use super::*;
use axum::body::Body;
use axum::http::Request as HttpRequest;
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

fn router(secret: &str) -> Router {
    Router::new()
        .route("/protected", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(secret.to_string(), require_internal_secret))
        .with_state(())
}

#[tokio::test]
async fn rejects_a_request_with_no_secret_header() {
    let response = router("shh")
        .oneshot(HttpRequest::builder().uri("/protected").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_a_request_with_the_wrong_secret_header() {
    let response = router("shh")
        .oneshot(
            HttpRequest::builder()
                .uri("/protected")
                .header("x-internal-secret", "wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn allows_a_request_with_the_correct_secret_header() {
    let response = router("shh")
        .oneshot(
            HttpRequest::builder()
                .uri("/protected")
                .header("x-internal-secret", "shh")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
