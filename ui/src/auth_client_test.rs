use super::*;
use axum::extract::{Json as JsonExtractor, State};
use axum::response::{IntoResponse, Json};
use axum::routing::post;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryAuthClient {
    pub logins: Mutex<Vec<(String, String, String)>>,
    pub result: Mutex<Option<(String, Uuid, Role)>>,
}

#[async_trait]
impl AuthClient for InMemoryAuthClient {
    async fn local_login(
        &self,
        tenant_name: &str,
        username: &str,
        password: &str,
    ) -> Result<(String, Uuid, Role), AuthClientError> {
        self.logins.lock().unwrap().push((
            tenant_name.to_string(),
            username.to_string(),
            password.to_string(),
        ));
        self.result.lock().unwrap().clone().ok_or(AuthClientError::InvalidCredentials)
    }
}

pub struct FailingAuthClient;

#[async_trait]
impl AuthClient for FailingAuthClient {
    async fn local_login(
        &self,
        _tenant_name: &str,
        _username: &str,
        _password: &str,
    ) -> Result<(String, Uuid, Role), AuthClientError> {
        Err(AuthClientError::Unreachable("simulated failure".to_string()))
    }
}

#[derive(Clone)]
struct ExpectedCreds {
    username: String,
    password: String,
    tenant_id: Uuid,
}

async fn spawn_stub_server(expected: ExpectedCreds) -> String {
    #[derive(serde::Deserialize)]
    struct Body {
        username: String,
        password: String,
    }
    async fn handler(
        State(expected): State<ExpectedCreds>,
        JsonExtractor(body): JsonExtractor<Body>,
    ) -> axum::response::Response {
        if body.username == expected.username && body.password == expected.password {
            Json(serde_json::json!({
                "token": "issued-token", "tenant_id": expected.tenant_id, "role": "operator"
            }))
            .into_response()
        } else {
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
    let app = Router::new().route("/v1/auth/local/login", post(handler)).with_state(expected);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_returns_the_token_and_tenant_id_on_valid_credentials() {
    let tenant_id = Uuid::new_v4();
    let url = spawn_stub_server(ExpectedCreds {
        username: "alice".to_string(),
        password: "correct-password".to_string(),
        tenant_id,
    })
    .await;
    let client = HttpAuthClient::new(reqwest::Client::new(), url);

    let (token, returned_tenant_id, role) =
        client.local_login("acme", "alice", "correct-password").await.unwrap();
    assert_eq!(token, "issued-token");
    assert_eq!(returned_tenant_id, tenant_id);
    assert_eq!(role, Role::Operator);
}

#[tokio::test]
async fn http_client_returns_invalid_credentials_on_401() {
    let url = spawn_stub_server(ExpectedCreds {
        username: "alice".to_string(),
        password: "correct-password".to_string(),
        tenant_id: Uuid::new_v4(),
    })
    .await;
    let client = HttpAuthClient::new(reqwest::Client::new(), url);

    let err = client.local_login("acme", "alice", "wrong").await.unwrap_err();
    assert!(matches!(err, AuthClientError::InvalidCredentials));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpAuthClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.local_login("acme", "alice", "correct-password").await.unwrap_err();
    assert!(matches!(err, AuthClientError::Unreachable(_)));
}
