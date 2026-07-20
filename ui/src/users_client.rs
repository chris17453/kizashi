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
}

#[derive(Debug, Error)]
pub enum UsersClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("auth service rejected the request: HTTP {0}")]
    Rejected(u16),
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
            return Err(UsersClientError::Rejected(response.status().as_u16()));
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
            return Err(UsersClientError::Rejected(response.status().as_u16()));
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
            return Err(UsersClientError::Rejected(response.status().as_u16()));
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
            return Err(UsersClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }
}
