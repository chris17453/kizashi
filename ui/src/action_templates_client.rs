#[path = "action_templates_client_test.rs"]
#[cfg(test)]
pub(crate) mod action_templates_client_test;

use async_trait::async_trait;
use common::ActionTemplate;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

static CLIENT: OnceLock<Arc<dyn ActionTemplatesClient>> = OnceLock::new();

pub fn initialize(client: Arc<dyn ActionTemplatesClient>) {
    let _ = CLIENT.set(client);
}

pub fn global() -> Option<Arc<dyn ActionTemplatesClient>> {
    CLIENT.get().cloned()
}

#[derive(Debug, Error)]
pub enum ActionTemplatesClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[async_trait]
pub trait ActionTemplatesClient: Send + Sync {
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionTemplate>, ActionTemplatesClientError>;
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ActionTemplate>, ActionTemplatesClientError>;
    async fn create(
        &self,
        role: common::Role,
        actor: &str,
        template: ActionTemplate,
    ) -> Result<ActionTemplate, ActionTemplatesClientError>;
    async fn update(
        &self,
        role: common::Role,
        actor: &str,
        template: ActionTemplate,
    ) -> Result<ActionTemplate, ActionTemplatesClientError>;
    async fn delete(
        &self,
        role: common::Role,
        actor: &str,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), ActionTemplatesClientError>;
}

pub struct HttpActionTemplatesClient {
    client: reqwest::Client,
    config_admin_service_url: String,
}

impl HttpActionTemplatesClient {
    pub fn new(client: reqwest::Client, config_admin_service_url: String) -> Self {
        Self { client, config_admin_service_url }
    }

    async fn response<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, ActionTemplatesClientError> {
        let status = response.status();
        if !status.is_success() {
            return Err(ActionTemplatesClientError::Rejected(status.as_u16()));
        }
        response
            .json()
            .await
            .map_err(|error| ActionTemplatesClientError::Unreachable(error.to_string()))
    }
}

#[async_trait]
impl ActionTemplatesClient for HttpActionTemplatesClient {
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionTemplate>, ActionTemplatesClientError> {
        let response = self
            .client
            .get(format!("{}/v1/action-templates", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|error| ActionTemplatesClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ActionTemplate>, ActionTemplatesClientError> {
        let response = self
            .client
            .get(format!("{}/v1/action-templates/{id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|error| ActionTemplatesClientError::Unreachable(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::response(response).await.map(Some)
    }

    async fn create(
        &self,
        role: common::Role,
        actor: &str,
        template: ActionTemplate,
    ) -> Result<ActionTemplate, ActionTemplatesClientError> {
        let response = self
            .client
            .post(format!("{}/v1/action-templates", self.config_admin_service_url))
            .header("x-tenant-id", template.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&template)
            .send()
            .await
            .map_err(|error| ActionTemplatesClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }

    async fn update(
        &self,
        role: common::Role,
        actor: &str,
        template: ActionTemplate,
    ) -> Result<ActionTemplate, ActionTemplatesClientError> {
        let response = self
            .client
            .put(format!("{}/v1/action-templates/{}", self.config_admin_service_url, template.id))
            .header("x-tenant-id", template.tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&template)
            .send()
            .await
            .map_err(|error| ActionTemplatesClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }

    async fn delete(
        &self,
        role: common::Role,
        actor: &str,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), ActionTemplatesClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/action-templates/{id}", self.config_admin_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|error| ActionTemplatesClientError::Unreachable(error.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ActionTemplatesClientError::Rejected(response.status().as_u16()))
        }
    }
}
