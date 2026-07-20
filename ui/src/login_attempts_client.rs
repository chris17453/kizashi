#[path = "login_attempts_client_test.rs"]
#[cfg(test)]
pub(crate) mod login_attempts_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct LoginAttempt {
    pub username: String,
    pub success: bool,
    pub reason: String,
    pub attempted_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum LoginAttemptsClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("auth service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Console UI's client for Auth Service's login-attempt telemetry (ADR-0053) --
/// `GET /v1/auth/local/login-attempts`, Admin-only on the backend, same direct-call trust
/// boundary as `UsersClient`.
#[async_trait]
pub trait LoginAttemptsClient: Send + Sync {
    /// `before` is the same exclusive keyset-pagination cursor
    /// `LoginAttemptRepository::list_recent` (auth-service) already implements server-side --
    /// pass the oldest `attempted_at` seen so far to page back through a busy tenant's history
    /// instead of only ever seeing the newest page (ADR-0063).
    async fn list_recent(
        &self,
        tenant_id: Uuid,
        role: Role,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptsClientError>;
}

pub struct HttpLoginAttemptsClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpLoginAttemptsClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

#[async_trait]
impl LoginAttemptsClient for HttpLoginAttemptsClient {
    async fn list_recent(
        &self,
        tenant_id: Uuid,
        role: Role,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptsClientError> {
        let mut request = self
            .client
            .get(format!("{}/v1/auth/local/login-attempts", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string());
        if let Some(before) = before {
            request = request.query(&[("before", before.to_rfc3339())]);
        }
        let response = request
            .send()
            .await
            .map_err(|e| LoginAttemptsClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LoginAttemptsClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| LoginAttemptsClientError::Unreachable(e.to_string()))
    }
}
