use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

#[tokio::test]
async fn get_charts_js_returns_javascript_content_type() {
    let app = Router::new().route("/static/charts.js", get(get_charts_js));
    let response = app
        .oneshot(Request::builder().uri("/static/charts.js").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("renderBarChart"));
}
