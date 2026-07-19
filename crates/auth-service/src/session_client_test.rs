use super::*;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySessionClient {
    pub minted: Mutex<Vec<(Uuid, Role, String)>>,
}

#[async_trait]
impl SessionClient for InMemorySessionClient {
    async fn mint_session(
        &self,
        tenant_id: Uuid,
        role: Role,
        label: &str,
    ) -> Result<String, SessionClientError> {
        self.minted.lock().unwrap().push((tenant_id, role, label.to_string()));
        Ok(format!("session-for-{tenant_id}"))
    }
}

pub struct FailingSessionClient;

#[async_trait]
impl SessionClient for FailingSessionClient {
    async fn mint_session(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _label: &str,
    ) -> Result<String, SessionClientError> {
        Err(SessionClientError::Unreachable("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_client_records_minted_sessions() {
    let client = InMemorySessionClient::default();
    let tenant_id = Uuid::new_v4();

    let token = client.mint_session(tenant_id, Role::Admin, "local-login").await.unwrap();

    assert_eq!(token, format!("session-for-{tenant_id}"));
    assert_eq!(client.minted.lock().unwrap().len(), 1);
}

async fn spawn_stub_query_gateway(status: axum::http::StatusCode) -> String {
    async fn handler(
        axum::extract::State(status): State<axum::http::StatusCode>,
        Json(_body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if status.is_success() {
            Json(serde_json::json!({"token": "minted-token"})).into_response()
        } else {
            status.into_response()
        }
    }
    let app = Router::new().route("/internal/tokens", post(handler)).with_state(status);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_parses_a_successful_response() {
    let url = spawn_stub_query_gateway(axum::http::StatusCode::CREATED).await;
    let client = HttpSessionClient::new(reqwest::Client::new(), url, "secret".to_string());

    let token = client.mint_session(Uuid::new_v4(), Role::Operator, "local-login").await.unwrap();
    assert_eq!(token, "minted-token");
}

#[tokio::test]
async fn http_client_returns_rejected_on_error_status() {
    let url = spawn_stub_query_gateway(axum::http::StatusCode::UNAUTHORIZED).await;
    let client = HttpSessionClient::new(reqwest::Client::new(), url, "secret".to_string());

    let err = client.mint_session(Uuid::new_v4(), Role::Operator, "local-login").await.unwrap_err();
    assert!(matches!(err, SessionClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpSessionClient::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1".to_string(),
        "secret".to_string(),
    );
    let err = client.mint_session(Uuid::new_v4(), Role::Operator, "local-login").await.unwrap_err();
    assert!(matches!(err, SessionClientError::Unreachable(_)));
}
