#[path = "users_client_test.rs"]
#[cfg(test)]
pub(crate) mod users_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct UiUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub role: Role,
    #[serde(default)]
    pub mfa_enabled: bool,
}

/// The live-enforced password policy parameters (ADR-0056), for the compliance report to
/// describe accurately rather than hardcoding a copy that could drift from what
/// `password_policy::validate_password_strength` actually enforces.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct PasswordPolicySummary {
    pub min_length: usize,
    pub max_length: usize,
    pub blocklist_size: usize,
}

#[derive(Debug, Error)]
pub enum UsersClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    /// `message` is the backend's own `{"error": "..."}` body when present (e.g. a password
    /// policy rejection) -- surfacing it lets an admin see *why* a create/update was rejected
    /// instead of just an HTTP status code, ADR-0052.
    #[error("auth service rejected the request: HTTP {status}: {message}")]
    Rejected { status: u16, message: String },
}

/// User management/role-assignment (ADR-0016 follow-up: the "assign role to another user"
/// surface explicitly deferred by RBAC v1) via Auth Service's `/v1/users` — same direct-call
/// trust boundary as every other write-path client (no gateway in front, ADR-0010), Admin-only
/// on the backend.
#[async_trait]
pub trait UsersClient: Send + Sync {
    async fn list_users(
        &self,
        tenant_id: Uuid,
        role: Role,
    ) -> Result<Vec<UiUser>, UsersClientError>;

    async fn create_user(
        &self,
        tenant_id: Uuid,
        role: Role,
        username: &str,
        password: &str,
        new_user_role: Role,
        actor: &str,
    ) -> Result<UiUser, UsersClientError>;

    async fn update_user_role(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
        new_role: Role,
        actor: &str,
    ) -> Result<UiUser, UsersClientError>;

    async fn delete_user(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
        actor: &str,
    ) -> Result<(), UsersClientError>;

    /// Data subject export (ADR-0054) — the raw JSON body from Auth Service's
    /// `GET /v1/users/:id/data-subject-export` (account record, its audit trail, and its login
    /// attempts), passed through as bytes rather than re-modeled here, since the Console UI only
    /// needs to hand it to the admin as a downloadable file.
    async fn export_user_data(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
    ) -> Result<Vec<u8>, UsersClientError>;

    async fn password_policy(&self) -> Result<PasswordPolicySummary, UsersClientError>;

    /// Self-service password change (ADR-0057) — `POST /v1/auth/local/password`, requires the
    /// caller's own current password (not just an authenticated session), same trust boundary
    /// as `MfaClient::disable`.
    async fn change_password(
        &self,
        tenant_id: Uuid,
        username: &str,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), UsersClientError>;
}

/// Reads the backend's `{"error": "..."}` body when present, falling back to a generic message
/// if the response isn't JSON-shaped that way (e.g. a proxy/gateway error page).
async fn rejected_error(response: reqwest::Response) -> UsersClientError {
    let status = response.status().as_u16();
    #[derive(serde::Deserialize)]
    struct ErrorBody {
        error: String,
    }
    let message = response
        .json::<ErrorBody>()
        .await
        .map(|body| body.error)
        .unwrap_or_else(|_| "no further detail available".to_string());
    UsersClientError::Rejected { status, message }
}

pub struct HttpUsersClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpUsersClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

#[async_trait]
impl UsersClient for HttpUsersClient {
    async fn list_users(
        &self,
        tenant_id: Uuid,
        role: Role,
    ) -> Result<Vec<UiUser>, UsersClientError> {
        let response = self
            .client
            .get(format!("{}/v1/users", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        response.json().await.map_err(|e| UsersClientError::Unreachable(e.to_string()))
    }

    async fn create_user(
        &self,
        tenant_id: Uuid,
        role: Role,
        username: &str,
        password: &str,
        new_user_role: Role,
        actor: &str,
    ) -> Result<UiUser, UsersClientError> {
        let response = self
            .client
            .post(format!("{}/v1/users", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({
                "username": username,
                "password": password,
                "role": new_user_role,
            }))
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        response.json().await.map_err(|e| UsersClientError::Unreachable(e.to_string()))
    }

    async fn update_user_role(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
        new_role: Role,
        actor: &str,
    ) -> Result<UiUser, UsersClientError> {
        let response = self
            .client
            .put(format!("{}/v1/users/{id}", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({ "role": new_role }))
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        response.json().await.map_err(|e| UsersClientError::Unreachable(e.to_string()))
    }

    async fn delete_user(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
        actor: &str,
    ) -> Result<(), UsersClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/users/{id}", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        Ok(())
    }

    async fn export_user_data(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
    ) -> Result<Vec<u8>, UsersClientError> {
        let response = self
            .client
            .get(format!("{}/v1/users/{id}/data-subject-export", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))
    }

    async fn password_policy(&self) -> Result<PasswordPolicySummary, UsersClientError> {
        let response = self
            .client
            .get(format!("{}/v1/auth/local/password-policy", self.auth_service_url))
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        response.json().await.map_err(|e| UsersClientError::Unreachable(e.to_string()))
    }

    async fn change_password(
        &self,
        tenant_id: Uuid,
        username: &str,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), UsersClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/password", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-username", username)
            .json(&serde_json::json!({
                "current_password": current_password,
                "new_password": new_password,
            }))
            .send()
            .await
            .map_err(|e| UsersClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(rejected_error(response).await);
        }
        Ok(())
    }
}
