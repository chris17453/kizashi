use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_200() {
    let app: Router = Router::new().route("/healthz", get(healthz));
    let response =
        app.oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
