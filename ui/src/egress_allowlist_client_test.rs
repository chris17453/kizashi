use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryEgressAllowlistClient {
    pub domains: Mutex<HashMap<Uuid, Vec<String>>>,
}

#[async_trait]
impl EgressAllowlistClient for InMemoryEgressAllowlistClient {
    async fn get_allowlist(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, EgressAllowlistClientError> {
        Ok(self.domains.lock().unwrap().get(&tenant_id).cloned().unwrap_or_default())
    }

    async fn put_allowlist(
        &self,
        tenant_id: Uuid,
        role: Role,
        domains: Vec<String>,
        _actor: &str,
    ) -> Result<Vec<String>, EgressAllowlistClientError> {
        if !role.at_least(Role::Operator) {
            return Err(EgressAllowlistClientError::Rejected(403));
        }
        self.domains.lock().unwrap().insert(tenant_id, domains.clone());
        Ok(domains)
    }
}

pub struct FailingEgressAllowlistClient;

#[async_trait]
impl EgressAllowlistClient for FailingEgressAllowlistClient {
    async fn get_allowlist(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<String>, EgressAllowlistClientError> {
        Err(EgressAllowlistClientError::Unreachable("simulated failure".to_string()))
    }

    async fn put_allowlist(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _domains: Vec<String>,
        _actor: &str,
    ) -> Result<Vec<String>, EgressAllowlistClientError> {
        Err(EgressAllowlistClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn get_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!(["zendesk.com"])).into_response()
    }
    async fn put_handler(
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        if headers.get("x-username").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(body["domains"].clone()).into_response()
    }
    let app = Router::new().route("/v1/allowlist", get(get_handler).put(put_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_gets_the_configured_allowlist_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpEgressAllowlistClient::new(reqwest::Client::new(), url);

    let domains = client.get_allowlist(Uuid::new_v4()).await.unwrap();
    assert_eq!(domains, vec!["zendesk.com".to_string()]);
}

#[tokio::test]
async fn http_client_puts_a_new_allowlist_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpEgressAllowlistClient::new(reqwest::Client::new(), url);

    let domains = client
        .put_allowlist(
            Uuid::new_v4(),
            Role::Operator,
            vec!["api.github.com".to_string(), "example.com".to_string()],
            "alice",
        )
        .await
        .unwrap();
    assert_eq!(domains, vec!["api.github.com".to_string(), "example.com".to_string()]);
}

#[tokio::test]
async fn http_client_put_is_rejected_for_insufficient_role() {
    let url = spawn_stub_server().await;
    let client = HttpEgressAllowlistClient::new(reqwest::Client::new(), url);

    let err = client
        .put_allowlist(Uuid::new_v4(), Role::Viewer, vec!["x.com".to_string()], "alice")
        .await
        .unwrap_err();
    assert!(matches!(err, EgressAllowlistClientError::Rejected(403)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpEgressAllowlistClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.get_allowlist(Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, EgressAllowlistClientError::Unreachable(_)));
}
