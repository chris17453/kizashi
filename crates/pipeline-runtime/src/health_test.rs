use super::*;

#[tokio::test]
async fn healthz_returns_ok() {
    assert_eq!(healthz().await, axum::http::StatusCode::OK);
}
