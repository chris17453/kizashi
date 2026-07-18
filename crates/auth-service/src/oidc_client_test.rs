use super::*;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryOidcClient {
    pub exchanged_codes: Mutex<Vec<String>>,
}

#[async_trait]
impl OidcClient for InMemoryOidcClient {
    fn authorization_request(&self) -> Result<AuthorizationRequest, OidcError> {
        Ok(AuthorizationRequest {
            authorization_url: "https://idp.test/authorize?...".to_string(),
            csrf_token: "test-csrf".to_string(),
            code_verifier: "test-verifier".to_string(),
        })
    }

    async fn exchange_code(&self, code: &str, _code_verifier: &str) -> Result<String, OidcError> {
        self.exchanged_codes.lock().unwrap().push(code.to_string());
        Ok("test-access-token".to_string())
    }

    async fn fetch_userinfo(&self, _access_token: &str) -> Result<OidcUserInfo, OidcError> {
        Ok(OidcUserInfo {
            subject: "test-subject".to_string(),
            email: Some("user@example.test".to_string()),
        })
    }
}

pub struct FailingOidcClient;

#[async_trait]
impl OidcClient for FailingOidcClient {
    fn authorization_request(&self) -> Result<AuthorizationRequest, OidcError> {
        Err(OidcError::Config("simulated failure".to_string()))
    }

    async fn exchange_code(&self, _code: &str, _code_verifier: &str) -> Result<String, OidcError> {
        Err(OidcError::Exchange("simulated failure".to_string()))
    }

    async fn fetch_userinfo(&self, _access_token: &str) -> Result<OidcUserInfo, OidcError> {
        Err(OidcError::Userinfo("simulated failure".to_string()))
    }
}

#[test]
fn authorization_request_returns_a_url_state_and_verifier() {
    let client = InMemoryOidcClient::default();
    let req = client.authorization_request().unwrap();
    assert!(!req.authorization_url.is_empty());
    assert!(!req.code_verifier.is_empty());
}

/// Spins up axum handlers implementing the standard OIDC `/token` and `/userinfo` endpoints,
/// so `StandardOidcClient`'s real code-exchange and userinfo-fetch logic is exercised against
/// something that actually speaks the protocol, not just an in-memory double. What this test
/// deliberately does not cover is the human browser hop between "redirect to authorization_url"
/// and "IdP redirects back with a code" — that step is inherent to OIDC and cannot be
/// meaningfully automated without a browser driver against a real or fake IdP UI (ADR-0009).
async fn spawn_stub_oidc_provider() -> String {
    async fn token_handler(
        Form(_params): Form<std::collections::HashMap<String, String>>,
    ) -> axum::response::Response {
        Json(serde_json::json!({
            "access_token": "stub-access-token",
            "token_type": "Bearer",
            "expires_in": 3600
        }))
        .into_response()
    }
    async fn userinfo_handler(headers: axum::http::HeaderMap) -> axum::response::Response {
        let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
        if auth == "Bearer stub-access-token" {
            Json(serde_json::json!({"sub": "stub-subject", "email": "stub@example.test"}))
                .into_response()
        } else {
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
    let app = Router::new()
        .route("/token", post(token_handler))
        .route("/userinfo", get(userinfo_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn test_config(base_url: &str) -> OidcProviderConfig {
    OidcProviderConfig {
        client_id: "test-client".to_string(),
        client_secret: "test-secret".to_string(),
        auth_url: format!("{base_url}/authorize"),
        token_url: format!("{base_url}/token"),
        userinfo_url: format!("{base_url}/userinfo"),
        redirect_url: "http://localhost/callback".to_string(),
    }
}

#[tokio::test]
async fn standard_client_builds_a_valid_authorization_request() {
    let base_url = spawn_stub_oidc_provider().await;
    let client = StandardOidcClient::new(test_config(&base_url)).unwrap();

    let req = client.authorization_request().unwrap();
    assert!(req.authorization_url.starts_with(&format!("{base_url}/authorize")));
    assert!(!req.code_verifier.is_empty());
    assert!(!req.csrf_token.is_empty());
}

#[tokio::test]
async fn standard_client_exchanges_a_code_for_an_access_token() {
    let base_url = spawn_stub_oidc_provider().await;
    let client = StandardOidcClient::new(test_config(&base_url)).unwrap();
    let req = client.authorization_request().unwrap();

    let access_token = client.exchange_code("stub-auth-code", &req.code_verifier).await.unwrap();
    assert_eq!(access_token, "stub-access-token");
}

#[tokio::test]
async fn standard_client_fetches_userinfo_with_the_access_token() {
    let base_url = spawn_stub_oidc_provider().await;
    let client = StandardOidcClient::new(test_config(&base_url)).unwrap();

    let info = client.fetch_userinfo("stub-access-token").await.unwrap();
    assert_eq!(info.subject, "stub-subject");
    assert_eq!(info.email, Some("stub@example.test".to_string()));
}

#[tokio::test]
async fn standard_client_userinfo_fails_with_an_invalid_token() {
    let base_url = spawn_stub_oidc_provider().await;
    let client = StandardOidcClient::new(test_config(&base_url)).unwrap();

    let err = client.fetch_userinfo("wrong-token").await.unwrap_err();
    assert!(matches!(err, OidcError::Userinfo(_)));
}

#[test]
fn new_rejects_an_invalid_auth_url() {
    let mut config = test_config("http://localhost");
    config.auth_url = "not a url".to_string();
    match StandardOidcClient::new(config) {
        Err(OidcError::Config(_)) => {}
        other => panic!("expected OidcError::Config, got {}", other.is_ok()),
    }
}
