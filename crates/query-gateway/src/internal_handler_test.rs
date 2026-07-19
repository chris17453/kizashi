use super::*;
use crate::token_store::token_store_test::{FailingTokenStore, InMemoryTokenStore};
use axum::body::Body;
use axum::http::Request;
use axum::routing::post;
use axum::Router;
use tower::ServiceExt;

fn state(
    internal_secret: &str,
    token_store: std::sync::Arc<dyn crate::token_store::TokenStore>,
) -> GatewayState {
    GatewayState {
        token_store,
        http_client: reqwest::Client::new(),
        dashboard_api_url: "http://unused".to_string(),
        internal_secret: internal_secret.to_string(),
    }
}

fn router(state: GatewayState) -> Router {
    Router::new().route("/internal/tokens", post(mint_token)).with_state(state)
}

#[tokio::test]
async fn mints_a_token_when_the_internal_secret_matches() {
    let store = std::sync::Arc::new(InMemoryTokenStore::default());
    let app_state = state("shared-secret", store);
    let tenant_id = Uuid::new_v4();

    let response = router(app_state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/tokens")
                .header("content-type", "application/json")
                .header("x-internal-secret", "shared-secret")
                .body(Body::from(
                    serde_json::json!({"tenant_id": tenant_id, "role": "operator", "label": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: MintTokenResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(!parsed.token.is_empty());
}

#[tokio::test]
async fn rejects_a_wrong_internal_secret() {
    let store = std::sync::Arc::new(InMemoryTokenStore::default());
    let app_state = state("shared-secret", store);

    let response = router(app_state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/tokens")
                .header("content-type", "application/json")
                .header("x-internal-secret", "wrong-secret")
                .body(Body::from(
                    serde_json::json!({"tenant_id": Uuid::new_v4(), "role": "operator", "label": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_a_missing_internal_secret() {
    let store = std::sync::Arc::new(InMemoryTokenStore::default());
    let app_state = state("shared-secret", store);

    let response = router(app_state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/tokens")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"tenant_id": Uuid::new_v4(), "role": "operator", "label": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn returns_500_on_token_store_failure() {
    let app_state = state("shared-secret", std::sync::Arc::new(FailingTokenStore));

    let response = router(app_state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/tokens")
                .header("content-type", "application/json")
                .header("x-internal-secret", "shared-secret")
                .body(Body::from(
                    serde_json::json!({"tenant_id": Uuid::new_v4(), "role": "operator", "label": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
