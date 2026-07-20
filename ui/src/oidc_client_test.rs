use super::*;
use axum::extract::{Json as JsonExtractor, Path};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryOidcClient {
    pub authorize_calls: Mutex<Vec<String>>,
    pub callback_calls: Mutex<Vec<(String, String, String, String)>>,
    pub authorize_result: Mutex<Option<OidcAuthorization>>,
    pub callback_result: Mutex<Option<OidcSession>>,
}

#[async_trait]
impl OidcClient for InMemoryOidcClient {
    async fn authorize(&self, provider: &str) -> Result<OidcAuthorization, OidcClientError> {
        self.authorize_calls.lock().unwrap().push(provider.to_string());
        self.authorize_result.lock().unwrap().clone().ok_or(OidcClientError::UnknownProvider)
    }

    async fn callback(
        &self,
        provider: &str,
        code: &str,
        code_verifier: &str,
        tenant_name: &str,
    ) -> Result<OidcSession, OidcClientError> {
        self.callback_calls.lock().unwrap().push((
            provider.to_string(),
            code.to_string(),
            code_verifier.to_string(),
            tenant_name.to_string(),
        ));
        self.callback_result.lock().unwrap().clone().ok_or(OidcClientError::UnknownWorkspace)
    }
}

pub struct FailingOidcClient;

#[async_trait]
impl OidcClient for FailingOidcClient {
    async fn authorize(&self, _provider: &str) -> Result<OidcAuthorization, OidcClientError> {
        Err(OidcClientError::Unreachable("simulated failure".to_string()))
    }

    async fn callback(
        &self,
        _provider: &str,
        _code: &str,
        _code_verifier: &str,
        _tenant_name: &str,
    ) -> Result<OidcSession, OidcClientError> {
        Err(OidcClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn authorize_handler(Path(provider): Path<String>) -> axum::response::Response {
        if provider != "entra" {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        }
        Json(serde_json::json!({
            "authorization_url": "https://idp.example.com/authorize?client_id=abc",
            "csrf_token": "csrf-123",
            "code_verifier": "verifier-123"
        }))
        .into_response()
    }

    #[derive(serde::Deserialize)]
    struct CallbackBody {
        code: String,
        code_verifier: String,
        tenant_name: String,
    }
    async fn callback_handler(
        Path(provider): Path<String>,
        JsonExtractor(body): JsonExtractor<CallbackBody>,
    ) -> axum::response::Response {
        if provider != "entra" {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        }
        if body.tenant_name != "acme" {
            return axum::http::StatusCode::BAD_REQUEST.into_response();
        }
        if body.code != "good-code" || body.code_verifier != "verifier-123" {
            return axum::http::StatusCode::BAD_GATEWAY.into_response();
        }
        Json(serde_json::json!({
            "token": "issued-token",
            "tenant_id": "00000000-0000-0000-0000-000000000042",
            "role": "viewer",
            "username": "alice@example.com"
        }))
        .into_response()
    }

    let app = Router::new()
        .route("/v1/auth/oidc/:provider/authorize", get(authorize_handler))
        .route("/v1/auth/oidc/:provider/callback", post(callback_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_authorize_returns_url_and_verifier() {
    let url = spawn_stub_server().await;
    let client = HttpOidcClient::new(reqwest::Client::new(), url);

    let result = client.authorize("entra").await.unwrap();
    assert!(!result.authorization_url.is_empty());
    assert!(!result.code_verifier.is_empty());
}

#[tokio::test]
async fn http_client_authorize_returns_unknown_provider_on_404() {
    let url = spawn_stub_server().await;
    let client = HttpOidcClient::new(reqwest::Client::new(), url);

    let err = client.authorize("nonexistent").await.unwrap_err();
    assert!(matches!(err, OidcClientError::UnknownProvider));
}

#[tokio::test]
async fn http_client_callback_returns_a_session_on_success() {
    let url = spawn_stub_server().await;
    let client = HttpOidcClient::new(reqwest::Client::new(), url);

    let session = client.callback("entra", "good-code", "verifier-123", "acme").await.unwrap();
    assert_eq!(session.bearer_token, "issued-token");
    assert_eq!(session.role, Role::Viewer);
    assert_eq!(session.username.as_deref(), Some("alice@example.com"));
}

#[tokio::test]
async fn http_client_callback_returns_unknown_workspace_on_400() {
    let url = spawn_stub_server().await;
    let client = HttpOidcClient::new(reqwest::Client::new(), url);

    let err =
        client.callback("entra", "good-code", "verifier-123", "nonexistent").await.unwrap_err();
    assert!(matches!(err, OidcClientError::UnknownWorkspace));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpOidcClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.authorize("entra").await.unwrap_err();
    assert!(matches!(err, OidcClientError::Unreachable(_)));
}
