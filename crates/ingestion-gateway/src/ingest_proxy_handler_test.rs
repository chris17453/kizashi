use super::*;
use crate::agent_status_client::agent_status_client_test::{
    FailingAgentStatusClient, InMemoryAgentStatusClient,
};
use crate::api_key_store::api_key_store_test::InMemoryApiKeyStore;
use crate::rate_limiter::rate_limiter_test::TestClock;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

async fn spawn_stub_ingestion_service() -> String {
    async fn stub_ingest(body: axum::body::Bytes) -> Response {
        let received: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (StatusCode::CREATED, Json(serde_json::json!({"id": Uuid::new_v4(), "received": received})))
            .into_response()
    }

    let app = Router::new().route("/v1/records", post(stub_ingest));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn gateway_state(
    ingestion_service_url: String,
    api_key: &str,
    tenant_id: Uuid,
    rate_limit: u32,
) -> GatewayState {
    GatewayState {
        api_key_store: Arc::new(InMemoryApiKeyStore::with_key(api_key, tenant_id)),
        audit_reader: Arc::new(crate::audit_log::audit_log_test::InMemoryAuditLogReader::default()),
        rate_limiter: Arc::new(RateLimiter::new(
            rate_limit,
            Duration::from_secs(60),
            Box::new(TestClock::new()),
        )),
        http_client: reqwest::Client::new(),
        ingestion_service_url,
        agent_status_client: Arc::new(InMemoryAgentStatusClient::default()),
    }
}

fn router(state: GatewayState) -> Router {
    Router::new().route("/v1/ingest", post(ingest_proxy)).with_state(state)
}

async fn post_with_key(
    app: Router,
    api_key: Option<&str>,
    body: serde_json::Value,
) -> axum::http::Response<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri("/v1/ingest")
        .header("content-type", "application/json");
    if let Some(key) = api_key {
        req = req.header("x-api-key", key);
    }
    app.oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap()
}

#[tokio::test]
async fn valid_key_forwards_to_ingestion_service_with_authenticated_tenant_id() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let tenant_id = Uuid::new_v4();
    let state = gateway_state(upstream_url, "valid-key", tenant_id, 10);

    let body = serde_json::json!({
        "connector_id": "zendesk",
        "source_type": "ticket",
        "tenant_id": Uuid::nil(), // client-claimed tenant_id must be overridden
        "raw_payload": {"subject": "help"},
    });

    let response = post_with_key(router(state), Some("valid-key"), body).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed["received"]["tenant_id"], serde_json::json!(tenant_id));
}

#[tokio::test]
async fn missing_api_key_is_rejected_with_401() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let state = gateway_state(upstream_url, "valid-key", Uuid::new_v4(), 10);

    let response = post_with_key(router(state), None, serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_api_key_is_rejected_with_401() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let state = gateway_state(upstream_url, "valid-key", Uuid::new_v4(), 10);

    let response = post_with_key(router(state), Some("wrong-key"), serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn exceeding_rate_limit_returns_429() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let tenant_id = Uuid::new_v4();
    let state = gateway_state(upstream_url, "valid-key", tenant_id, 1);
    let body =
        serde_json::json!({"connector_id": "zendesk", "source_type": "ticket", "raw_payload": {}});

    let first = post_with_key(router(state.clone()), Some("valid-key"), body.clone()).await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = post_with_key(router(state), Some("valid-key"), body).await;
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn non_json_body_is_rejected_with_400() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let state = gateway_state(upstream_url, "valid-key", Uuid::new_v4(), 10);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/ingest")
        .header("x-api-key", "valid-key")
        .body(Body::from("not json"))
        .unwrap();
    let response = router(state).oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upstream_unreachable_returns_502() {
    let tenant_id = Uuid::new_v4();
    let state = gateway_state("http://127.0.0.1:1".to_string(), "valid-key", tenant_id, 10);
    let body =
        serde_json::json!({"connector_id": "zendesk", "source_type": "ticket", "raw_payload": {}});

    let response = post_with_key(router(state), Some("valid-key"), body).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn ingestion_is_rejected_when_the_matching_agent_is_disabled() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let tenant_id = Uuid::new_v4();
    let mut state = gateway_state(upstream_url, "valid-key", tenant_id, 10);
    let status_client = Arc::new(InMemoryAgentStatusClient::default());
    *status_client.status.lock().unwrap() = AgentStatus::Disabled;
    state.agent_status_client = status_client;
    let body = serde_json::json!({"connector_id": "disabled-agent", "source_type": "ticket", "raw_payload": {}});

    let response = post_with_key(router(state), Some("valid-key"), body).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn ingestion_still_succeeds_when_the_agent_is_unregistered() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let tenant_id = Uuid::new_v4();
    // InMemoryAgentStatusClient defaults to AgentStatus::Unregistered — the permissive default
    // every connector without a registered Agent row relies on.
    let state = gateway_state(upstream_url, "valid-key", tenant_id, 10);
    let body = serde_json::json!({"connector_id": "ad-hoc-connector", "source_type": "ticket", "raw_payload": {}});

    let response = post_with_key(router(state), Some("valid-key"), body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn ingestion_still_succeeds_when_the_agent_status_lookup_fails() {
    let upstream_url = spawn_stub_ingestion_service().await;
    let tenant_id = Uuid::new_v4();
    let mut state = gateway_state(upstream_url, "valid-key", tenant_id, 10);
    state.agent_status_client = Arc::new(FailingAgentStatusClient);
    let body =
        serde_json::json!({"connector_id": "zendesk", "source_type": "ticket", "raw_payload": {}});

    // Availability of the ingest path matters more than this soft-enforcement check — a
    // config-admin-service blip must never take down ingestion for every connector.
    let response = post_with_key(router(state), Some("valid-key"), body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}
