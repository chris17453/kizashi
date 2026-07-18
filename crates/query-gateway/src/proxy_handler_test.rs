use super::*;
use crate::token_store::token_store_test::InMemoryTokenStore;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use uuid::Uuid;

async fn spawn_stub_dashboard_api() -> String {
    async fn handler(headers: HeaderMap) -> Response {
        let tenant_id =
            headers.get("x-tenant-id").and_then(|v| v.to_str().ok()).unwrap_or("missing");
        Json(serde_json::json!({"seen_tenant_id": tenant_id})).into_response()
    }
    let app = Router::new().route("/v1/events", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn gateway_state(dashboard_api_url: String, token: &str, tenant_id: Uuid) -> GatewayState {
    GatewayState {
        token_store: Arc::new(InMemoryTokenStore::with_token(token, tenant_id)),
        http_client: reqwest::Client::new(),
        dashboard_api_url,
        internal_secret: "test-internal-secret".to_string(),
    }
}

fn router(state: GatewayState) -> Router {
    Router::new().route("/v1/events", get(proxy_get)).with_state(state)
}

#[tokio::test]
async fn valid_token_forwards_with_resolved_tenant_id_header() {
    let upstream_url = spawn_stub_dashboard_api().await;
    let tenant_id = Uuid::new_v4();
    let state = gateway_state(upstream_url, "valid-token", tenant_id);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("authorization", "Bearer valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed["seen_tenant_id"], serde_json::json!(tenant_id.to_string()));
}

#[tokio::test]
async fn missing_authorization_header_is_rejected_with_401() {
    let upstream_url = spawn_stub_dashboard_api().await;
    let state = gateway_state(upstream_url, "valid-token", Uuid::new_v4());

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_token_is_rejected_with_401() {
    let upstream_url = spawn_stub_dashboard_api().await;
    let state = gateway_state(upstream_url, "valid-token", Uuid::new_v4());

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn upstream_unreachable_returns_502() {
    let tenant_id = Uuid::new_v4();
    let state = gateway_state("http://127.0.0.1:1".to_string(), "valid-token", tenant_id);

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("authorization", "Bearer valid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}
