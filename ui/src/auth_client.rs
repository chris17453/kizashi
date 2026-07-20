#[path = "auth_client_test.rs"]
#[cfg(test)]
pub(crate) mod auth_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuthClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("invalid credentials")]
    InvalidCredentials,
}

/// A successful `local_login` call either grants a session outright, or (ADR-0051) hands back a
/// short-lived `challenge_token` the caller must complete via `MfaClient::challenge` before a
/// real session exists — the password alone was correct, but that's only the first factor.
#[derive(Debug, Clone, PartialEq)]
pub enum LocalLoginResult {
    LoggedIn { token: String, tenant_id: Uuid, role: Role },
    MfaRequired { challenge_token: String },
}

/// Console UI's client for Auth Service's local-login endpoint — the browser never talks to
/// Auth Service directly, since session establishment (the `HttpOnly` cookie) is this
/// process's job (ADR-0014).
#[async_trait]
pub trait AuthClient: Send + Sync {
    async fn local_login(
        &self,
        tenant_name: &str,
        username: &str,
        password: &str,
    ) -> Result<LocalLoginResult, AuthClientError>;
}

pub struct HttpAuthClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpAuthClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

#[derive(serde::Deserialize)]
struct LoginResponse {
    token: Option<String>,
    tenant_id: Option<Uuid>,
    role: Option<Role>,
    #[serde(default)]
    mfa_required: bool,
    challenge_token: Option<String>,
}

#[async_trait]
impl AuthClient for HttpAuthClient {
    async fn local_login(
        &self,
        tenant_name: &str,
        username: &str,
        password: &str,
    ) -> Result<LocalLoginResult, AuthClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/login", self.auth_service_url))
            .json(&serde_json::json!({"tenant_name": tenant_name, "username": username, "password": password}))
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AuthClientError::InvalidCredentials);
        }
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }

        let body: LoginResponse =
            response.json().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))?;

        if body.mfa_required {
            let challenge_token = body.challenge_token.ok_or_else(|| {
                AuthClientError::Unreachable("missing challenge_token".to_string())
            })?;
            return Ok(LocalLoginResult::MfaRequired { challenge_token });
        }

        let (token, tenant_id, role) = match (body.token, body.tenant_id, body.role) {
            (Some(token), Some(tenant_id), Some(role)) => (token, tenant_id, role),
            _ => return Err(AuthClientError::Unreachable("incomplete login response".to_string())),
        };
        Ok(LocalLoginResult::LoggedIn { token, tenant_id, role })
    }
}
