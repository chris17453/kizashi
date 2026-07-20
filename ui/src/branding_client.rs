#[path = "branding_client_test.rs"]
#[cfg(test)]
pub(crate) mod branding_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BrandingClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("unknown workspace")]
    UnknownWorkspace,
    #[error("auth service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Branding {
    pub product_name: Option<String>,
    pub logo_url: Option<String>,
    pub accent_color: Option<String>,
}

/// Reads/writes a tenant's white-label overrides via Auth Service (spec §1: "white-labelable"),
/// which owns the `tenants` table this lives on. Read is by workspace name (the login page's
/// only identifier pre-authentication, ADR-0040); write is by id and admin-gated, same
/// direct-call trust boundary as every other Console UI client.
#[async_trait]
pub trait BrandingClient: Send + Sync {
    async fn get_branding(&self, tenant_name: &str) -> Result<Branding, BrandingClientError>;

    /// Used by the authenticated Settings page, which only ever has a `tenant_id` from the
    /// session, never the workspace name `get_branding` needs.
    async fn get_branding_by_id(&self, tenant_id: Uuid) -> Result<Branding, BrandingClientError>;

    async fn put_branding(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        branding: Branding,
    ) -> Result<(), BrandingClientError>;
}

pub struct HttpBrandingClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpBrandingClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

#[async_trait]
impl BrandingClient for HttpBrandingClient {
    async fn get_branding(&self, tenant_name: &str) -> Result<Branding, BrandingClientError> {
        let response = self
            .client
            .get(format!("{}/v1/tenants/{tenant_name}/branding", self.auth_service_url))
            .send()
            .await
            .map_err(|e| BrandingClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BrandingClientError::UnknownWorkspace);
        }
        if !response.status().is_success() {
            return Err(BrandingClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| BrandingClientError::Unreachable(e.to_string()))
    }

    async fn get_branding_by_id(&self, tenant_id: Uuid) -> Result<Branding, BrandingClientError> {
        let response = self
            .client
            .get(format!("{}/v1/tenants/id/{tenant_id}/branding", self.auth_service_url))
            .send()
            .await
            .map_err(|e| BrandingClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BrandingClientError::UnknownWorkspace);
        }
        if !response.status().is_success() {
            return Err(BrandingClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| BrandingClientError::Unreachable(e.to_string()))
    }

    async fn put_branding(
        &self,
        tenant_id: Uuid,
        role: Role,
        actor: &str,
        branding: Branding,
    ) -> Result<(), BrandingClientError> {
        let response = self
            .client
            .put(format!("{}/v1/tenants/{tenant_id}/branding", self.auth_service_url))
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&branding)
            .send()
            .await
            .map_err(|e| BrandingClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BrandingClientError::Rejected(response.status().as_u16()));
        }
        Ok(())
    }
}
