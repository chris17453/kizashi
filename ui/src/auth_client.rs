#[path = "auth_client_test.rs"]
#[cfg(test)]
pub(crate) mod auth_client_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuthClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("invalid credentials")]
    InvalidCredentials,
}

/// Console UI's client for Auth Service's local-login endpoint — the browser never talks to
/// Auth Service directly, since session establishment (the `HttpOnly` cookie) is this
/// process's job (ADR-0014).
#[async_trait]
pub trait AuthClient: Send + Sync {
    async fn local_login(
        &self,
        tenant_id: Uuid,
        username: &str,
        password: &str,
    ) -> Result<String, AuthClientError>;
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
    token: String,
}

#[async_trait]
impl AuthClient for HttpAuthClient {
    async fn local_login(
        &self,
        tenant_id: Uuid,
        username: &str,
        password: &str,
    ) -> Result<String, AuthClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/login", self.auth_service_url))
            .json(&serde_json::json!({"tenant_id": tenant_id, "username": username, "password": password}))
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
        Ok(body.token)
    }
}
