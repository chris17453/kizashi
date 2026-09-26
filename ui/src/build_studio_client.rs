#[path = "build_studio_client_test.rs"]
#[cfg(test)]
mod build_studio_client_test;

use async_trait::async_trait;
use common::{AppDefinition, DataSource, PipelineDefinition};
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

static CLIENT: OnceLock<Arc<dyn BuildStudioClient>> = OnceLock::new();
pub fn initialize(client: Arc<dyn BuildStudioClient>) {
    let _ = CLIENT.set(client);
}
pub fn global() -> Option<Arc<dyn BuildStudioClient>> {
    CLIENT.get().cloned()
}

#[derive(Debug, Error)]
pub enum BuildStudioClientError {
    #[error("config admin service unreachable: {0}")]
    Unreachable(String),
    #[error("config admin service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[async_trait]
pub trait BuildStudioClient: Send + Sync {
    async fn list_data_sources(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<DataSource>, BuildStudioClientError>;
    async fn list_pipelines(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<PipelineDefinition>, BuildStudioClientError>;
    async fn get_data_source(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DataSource>, BuildStudioClientError>;
    async fn create_data_source(
        &self,
        role: common::Role,
        actor: &str,
        source: DataSource,
    ) -> Result<DataSource, BuildStudioClientError>;
    async fn update_data_source(
        &self,
        role: common::Role,
        actor: &str,
        source: DataSource,
    ) -> Result<DataSource, BuildStudioClientError>;
    async fn get_pipeline(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PipelineDefinition>, BuildStudioClientError>;
    async fn create_pipeline(
        &self,
        role: common::Role,
        actor: &str,
        pipeline: PipelineDefinition,
    ) -> Result<PipelineDefinition, BuildStudioClientError>;
    async fn update_pipeline(
        &self,
        role: common::Role,
        actor: &str,
        pipeline: PipelineDefinition,
    ) -> Result<PipelineDefinition, BuildStudioClientError>;
    async fn list_apps(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<AppDefinition>, BuildStudioClientError>;
    async fn get_app(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<AppDefinition>, BuildStudioClientError>;
    async fn create_app(
        &self,
        role: common::Role,
        actor: &str,
        app: AppDefinition,
    ) -> Result<AppDefinition, BuildStudioClientError>;
    async fn update_app(
        &self,
        role: common::Role,
        actor: &str,
        app: AppDefinition,
    ) -> Result<AppDefinition, BuildStudioClientError>;
}

pub struct HttpBuildStudioClient {
    client: reqwest::Client,
    url: String,
    internal_secret: String,
}
impl HttpBuildStudioClient {
    pub fn new(client: reqwest::Client, url: String, internal_secret: String) -> Self {
        Self { client, url, internal_secret }
    }
    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        tenant_id: Uuid,
    ) -> Result<T, BuildStudioClientError> {
        let response = self
            .client
            .get(format!("{}/{}", self.url.trim_end_matches('/'), path))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(BuildStudioClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))
    }
    async fn response<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, BuildStudioClientError> {
        let status = response.status();
        if !status.is_success() {
            return Err(BuildStudioClientError::Rejected(status.as_u16()));
        }
        response
            .json()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))
    }
    fn request(
        &self,
        request: reqwest::RequestBuilder,
        role: common::Role,
        actor: &str,
    ) -> reqwest::RequestBuilder {
        request.header("x-role", role.to_string()).header("x-username", actor)
    }
}
#[async_trait]
impl BuildStudioClient for HttpBuildStudioClient {
    async fn list_data_sources(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<DataSource>, BuildStudioClientError> {
        self.get("v1/data-sources", tenant_id).await
    }
    async fn list_pipelines(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<PipelineDefinition>, BuildStudioClientError> {
        self.get("v1/pipeline-definitions", tenant_id).await
    }
    async fn get_data_source(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DataSource>, BuildStudioClientError> {
        let response = self
            .client
            .get(format!("{}/v1/data-sources/{id}", self.url.trim_end_matches('/')))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::response(response).await.map(Some)
    }
    async fn create_data_source(
        &self,
        role: common::Role,
        actor: &str,
        source: DataSource,
    ) -> Result<DataSource, BuildStudioClientError> {
        let request = self
            .client
            .post(format!("{}/v1/data-sources", self.url.trim_end_matches('/')))
            .header("x-tenant-id", source.tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .json(&source);
        let response = self
            .request(request, role, actor)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }
    async fn update_data_source(
        &self,
        role: common::Role,
        actor: &str,
        source: DataSource,
    ) -> Result<DataSource, BuildStudioClientError> {
        let request = self
            .client
            .put(format!("{}/v1/data-sources/{}", self.url.trim_end_matches('/'), source.id))
            .header("x-tenant-id", source.tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .json(&source);
        let response = self
            .request(request, role, actor)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }
    async fn get_pipeline(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PipelineDefinition>, BuildStudioClientError> {
        let response = self
            .client
            .get(format!("{}/v1/pipeline-definitions/{id}", self.url.trim_end_matches('/')))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::response(response).await.map(Some)
    }
    async fn create_pipeline(
        &self,
        role: common::Role,
        actor: &str,
        pipeline: PipelineDefinition,
    ) -> Result<PipelineDefinition, BuildStudioClientError> {
        let request = self
            .client
            .post(format!("{}/v1/pipeline-definitions", self.url.trim_end_matches('/')))
            .header("x-tenant-id", pipeline.tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .json(&pipeline);
        let response = self
            .request(request, role, actor)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }
    async fn update_pipeline(
        &self,
        role: common::Role,
        actor: &str,
        pipeline: PipelineDefinition,
    ) -> Result<PipelineDefinition, BuildStudioClientError> {
        let request = self
            .client
            .put(format!(
                "{}/v1/pipeline-definitions/{}",
                self.url.trim_end_matches('/'),
                pipeline.id
            ))
            .header("x-tenant-id", pipeline.tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .json(&pipeline);
        let response = self
            .request(request, role, actor)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }
    async fn list_apps(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<AppDefinition>, BuildStudioClientError> {
        self.get("v1/app-definitions", tenant_id).await
    }
    async fn get_app(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<AppDefinition>, BuildStudioClientError> {
        let response = self
            .client
            .get(format!("{}/v1/app-definitions/{id}", self.url.trim_end_matches('/')))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Self::response(response).await.map(Some)
    }
    async fn create_app(
        &self,
        role: common::Role,
        actor: &str,
        app: AppDefinition,
    ) -> Result<AppDefinition, BuildStudioClientError> {
        let request = self
            .client
            .post(format!("{}/v1/app-definitions", self.url.trim_end_matches('/')))
            .header("x-tenant-id", app.tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .json(&app);
        let response = self
            .request(request, role, actor)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }
    async fn update_app(
        &self,
        role: common::Role,
        actor: &str,
        app: AppDefinition,
    ) -> Result<AppDefinition, BuildStudioClientError> {
        let request = self
            .client
            .put(format!("{}/v1/app-definitions/{}", self.url.trim_end_matches('/'), app.id))
            .header("x-tenant-id", app.tenant_id.to_string())
            .header("x-internal-secret", &self.internal_secret)
            .json(&app);
        let response = self
            .request(request, role, actor)
            .send()
            .await
            .map_err(|error| BuildStudioClientError::Unreachable(error.to_string()))?;
        Self::response(response).await
    }
}
