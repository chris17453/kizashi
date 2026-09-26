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
    assert!(text.contains("renderLineChart"));
    assert!(text.contains("bindTooltip"));
    assert!(text.contains("chart-tooltip"));
}

#[tokio::test]
async fn get_confirm_danger_js_returns_javascript_content_type() {
    let app = Router::new().route("/static/confirm-danger.js", get(get_confirm_danger_js));
    let response = app
        .oneshot(Request::builder().uri("/static/confirm-danger.js").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("btn-danger"));
}

#[tokio::test]
async fn get_command_palette_js_returns_javascript_content_type() {
    let app = Router::new().route("/static/command-palette.js", get(get_command_palette_js));
    let response = app
        .oneshot(Request::builder().uri("/static/command-palette.js").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("command-palette"));
    assert!(text.contains("/ontology/objects/"));
    assert!(text.contains("data-investigation-route"));
    assert!(text.contains("/api/v1/search"));
    assert!(text.contains("/api/v1/saved-views?q="));
    assert!(text.contains("Saved view"));
    assert!(text.contains("/ontology/compare?ids="));
    assert!(text.contains("renderSearchResults"));
    assert!(text.contains("value.service"));
    assert!(text.contains("value.entry || value"));
    assert!(text.contains("/audit-log/"));
    assert!(text.contains("service === 'ingestion-gateway' ? 'ingestion'"));
    assert!(text.contains("service === 'egress-gateway' ? 'egress'"));
}
