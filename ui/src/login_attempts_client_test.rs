use super::*;
use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryLoginAttemptsClient {
    pub attempts: Mutex<Vec<LoginAttempt>>,
}

#[async_trait]
impl LoginAttemptsClient for InMemoryLoginAttemptsClient {
    async fn list_recent(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptsClientError> {
        Ok(self
            .attempts
            .lock()
            .unwrap()
            .iter()
            .filter(|a| before.is_none_or(|b| a.attempted_at < b))
            .cloned()
            .collect())
    }
}

pub struct FailingLoginAttemptsClient;

#[async_trait]
impl LoginAttemptsClient for FailingLoginAttemptsClient {
    async fn list_recent(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptsClientError> {
        Err(LoginAttemptsClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn handler(
        headers: HeaderMap,
        Query(query): Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        let username = if query.contains_key("before") { "older-alice" } else { "alice" };
        Json(serde_json::json!([{
            "username": username,
            "success": false,
            "reason": "wrong_password",
            "attempted_at": "2026-07-20T00:00:00Z"
        }]))
        .into_response()
    }
    let app = Router::new().route("/v1/auth/local/login-attempts", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_recent_attempts_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpLoginAttemptsClient::new(reqwest::Client::new(), url);

    let attempts = client.list_recent(Uuid::new_v4(), Role::Admin, None).await.unwrap();

    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].username, "alice");
    assert!(!attempts[0].success);
}

#[tokio::test]
async fn http_client_passes_the_before_cursor_as_a_query_param() {
    let url = spawn_stub_server().await;
    let client = HttpLoginAttemptsClient::new(reqwest::Client::new(), url);

    let attempts = client.list_recent(Uuid::new_v4(), Role::Admin, Some(Utc::now())).await.unwrap();

    assert_eq!(attempts[0].username, "older-alice");
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client =
        HttpLoginAttemptsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_recent(Uuid::new_v4(), Role::Admin, None).await.unwrap_err();
    assert!(matches!(err, LoginAttemptsClientError::Unreachable(_)));
}
