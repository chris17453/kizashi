use super::*;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;

pub struct InMemoryHealthClient {
    pub summary: PlatformHealthSummary,
}

#[async_trait]
impl HealthClient for InMemoryHealthClient {
    async fn platform_health(&self) -> Result<PlatformHealthSummary, HealthClientError> {
        Ok(self.summary.clone())
    }
}

pub struct FailingHealthClient;

#[async_trait]
impl HealthClient for FailingHealthClient {
    async fn platform_health(&self) -> Result<PlatformHealthSummary, HealthClientError> {
        Err(HealthClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server(status: axum::http::StatusCode) -> String {
    async fn handler() -> axum::response::Response {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "down", "services": [{"name": "svc-a", "status": "down"}]})),
        )
            .into_response()
    }
    async fn ok_handler() -> axum::response::Response {
        Json(serde_json::json!({"status": "up", "services": [{"name": "svc-a", "status": "up"}]}))
            .into_response()
    }
    let app = if status.is_success() {
        Router::new().route("/v1/health", get(ok_handler))
    } else {
        Router::new().route("/v1/health", get(handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_reads_health_on_200() {
    let url = spawn_stub_server(axum::http::StatusCode::OK).await;
    let client = HttpHealthClient::new(reqwest::Client::new(), url);

    let summary = client.platform_health().await.unwrap();
    assert_eq!(summary.status, "up");
}

#[tokio::test]
async fn http_client_still_parses_the_body_on_a_503() {
    let url = spawn_stub_server(axum::http::StatusCode::SERVICE_UNAVAILABLE).await;
    let client = HttpHealthClient::new(reqwest::Client::new(), url);

    let summary = client.platform_health().await.unwrap();
    assert_eq!(summary.status, "down");
    assert_eq!(summary.services[0].name, "svc-a");
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpHealthClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.platform_health().await.unwrap_err();
    assert!(matches!(err, HealthClientError::Unreachable(_)));
}
