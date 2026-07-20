use super::*;
use axum::extract::{Json as JsonExtractor, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::post;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryMfaClient {
    pub status_result: Mutex<bool>,
    pub enroll_result: Mutex<Option<MfaEnrollment>>,
    pub verify_should_fail: Mutex<bool>,
    pub disable_should_fail: Mutex<bool>,
    pub challenge_result: Mutex<Option<(String, Uuid, Role)>>,
}

#[async_trait]
impl MfaClient for InMemoryMfaClient {
    async fn status(&self, _tenant_id: Uuid, _username: &str) -> Result<bool, MfaClientError> {
        Ok(*self.status_result.lock().unwrap())
    }

    async fn enroll(
        &self,
        _tenant_id: Uuid,
        _username: &str,
    ) -> Result<MfaEnrollment, MfaClientError> {
        self.enroll_result.lock().unwrap().clone().ok_or(MfaClientError::Rejected(500))
    }

    async fn verify(
        &self,
        _tenant_id: Uuid,
        _username: &str,
        _code: &str,
    ) -> Result<(), MfaClientError> {
        if *self.verify_should_fail.lock().unwrap() {
            return Err(MfaClientError::Rejected(401));
        }
        Ok(())
    }

    async fn disable(
        &self,
        _tenant_id: Uuid,
        _username: &str,
        _password: &str,
    ) -> Result<(), MfaClientError> {
        if *self.disable_should_fail.lock().unwrap() {
            return Err(MfaClientError::Rejected(401));
        }
        Ok(())
    }

    async fn challenge(
        &self,
        _challenge_token: &str,
        _code: &str,
    ) -> Result<(String, Uuid, Role), MfaClientError> {
        self.challenge_result.lock().unwrap().clone().ok_or(MfaClientError::Rejected(401))
    }
}

pub struct FailingMfaClient;

#[async_trait]
impl MfaClient for FailingMfaClient {
    async fn status(&self, _tenant_id: Uuid, _username: &str) -> Result<bool, MfaClientError> {
        Err(MfaClientError::Unreachable("simulated failure".to_string()))
    }

    async fn enroll(
        &self,
        _tenant_id: Uuid,
        _username: &str,
    ) -> Result<MfaEnrollment, MfaClientError> {
        Err(MfaClientError::Unreachable("simulated failure".to_string()))
    }

    async fn verify(
        &self,
        _tenant_id: Uuid,
        _username: &str,
        _code: &str,
    ) -> Result<(), MfaClientError> {
        Err(MfaClientError::Unreachable("simulated failure".to_string()))
    }

    async fn disable(
        &self,
        _tenant_id: Uuid,
        _username: &str,
        _password: &str,
    ) -> Result<(), MfaClientError> {
        Err(MfaClientError::Unreachable("simulated failure".to_string()))
    }

    async fn challenge(
        &self,
        _challenge_token: &str,
        _code: &str,
    ) -> Result<(String, Uuid, Role), MfaClientError> {
        Err(MfaClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn status_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() || headers.get("x-username").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({"enabled": true})).into_response()
    }

    async fn enroll_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() || headers.get("x-username").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({
            "secret_base32": "SECRETBASE32",
            "provisioning_uri": "otpauth://totp/Kizashi:alice",
            "qr_code_base64_png": "aGVsbG8="
        }))
        .into_response()
    }

    #[derive(serde::Deserialize)]
    struct CodeBody {
        code: String,
    }
    async fn verify_handler(
        JsonExtractor(body): JsonExtractor<CodeBody>,
    ) -> axum::response::Response {
        if body.code == "123456" {
            axum::http::StatusCode::OK.into_response()
        } else {
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }

    #[derive(serde::Deserialize)]
    struct DisableBody {
        password: String,
    }
    async fn disable_handler(
        JsonExtractor(body): JsonExtractor<DisableBody>,
    ) -> axum::response::Response {
        if body.password == "correct-password" {
            axum::http::StatusCode::OK.into_response()
        } else {
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }

    #[derive(serde::Deserialize)]
    struct ChallengeBody {
        challenge_token: String,
        code: String,
    }
    async fn challenge_handler(
        State(tenant_id): State<Uuid>,
        JsonExtractor(body): JsonExtractor<ChallengeBody>,
    ) -> axum::response::Response {
        if body.challenge_token == "valid-token" && body.code == "123456" {
            Json(serde_json::json!({"token": "issued", "tenant_id": tenant_id, "role": "operator"}))
                .into_response()
        } else {
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }

    let tenant_id = Uuid::new_v4();
    let stateless_routes = Router::new()
        .route("/v1/auth/local/mfa/status", axum::routing::get(status_handler))
        .route("/v1/auth/local/mfa/enroll", post(enroll_handler))
        .route("/v1/auth/local/mfa/verify", post(verify_handler))
        .route("/v1/auth/local/mfa/disable", post(disable_handler));
    let challenge_route = Router::new()
        .route("/v1/auth/local/mfa/challenge", post(challenge_handler))
        .with_state(tenant_id);
    let app = stateless_routes.merge(challenge_route);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_enrolls_and_returns_the_provisioning_data() {
    let url = spawn_stub_server().await;
    let client = HttpMfaClient::new(reqwest::Client::new(), url);

    let enrollment = client.enroll(Uuid::new_v4(), "alice").await.unwrap();

    assert_eq!(enrollment.secret_base32, "SECRETBASE32");
    assert!(enrollment.provisioning_uri.starts_with("otpauth://"));
}

#[tokio::test]
async fn http_client_verify_succeeds_with_the_right_code() {
    let url = spawn_stub_server().await;
    let client = HttpMfaClient::new(reqwest::Client::new(), url);

    client.verify(Uuid::new_v4(), "alice", "123456").await.unwrap();
}

#[tokio::test]
async fn http_client_verify_fails_with_the_wrong_code() {
    let url = spawn_stub_server().await;
    let client = HttpMfaClient::new(reqwest::Client::new(), url);

    let err = client.verify(Uuid::new_v4(), "alice", "000000").await.unwrap_err();
    assert!(matches!(err, MfaClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_disable_succeeds_with_the_right_password() {
    let url = spawn_stub_server().await;
    let client = HttpMfaClient::new(reqwest::Client::new(), url);

    client.disable(Uuid::new_v4(), "alice", "correct-password").await.unwrap();
}

#[tokio::test]
async fn http_client_challenge_succeeds_with_the_right_token_and_code() {
    let url = spawn_stub_server().await;
    let client = HttpMfaClient::new(reqwest::Client::new(), url);

    let (token, _tenant_id, role) = client.challenge("valid-token", "123456").await.unwrap();

    assert_eq!(token, "issued");
    assert_eq!(role, Role::Operator);
}

#[tokio::test]
async fn http_client_challenge_returns_unreachable_when_server_is_down() {
    let client = HttpMfaClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.challenge("token", "123456").await.unwrap_err();
    assert!(matches!(err, MfaClientError::Unreachable(_)));
}

#[tokio::test]
async fn http_client_status_returns_the_current_enrollment_state() {
    let url = spawn_stub_server().await;
    let client = HttpMfaClient::new(reqwest::Client::new(), url);

    let enabled = client.status(Uuid::new_v4(), "alice").await.unwrap();

    assert!(enabled);
}
