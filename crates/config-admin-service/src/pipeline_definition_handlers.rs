#[cfg(test)]
#[path = "pipeline_definition_handlers_test.rs"]
mod pipeline_definition_handlers_test;

use crate::data_source_repository::DataSourceRepository;
use crate::handlers::{
    require_operator, tenant_id_from_headers, tenant_mismatch, username_from_headers,
};
use crate::pipeline_definition_repository::{
    PipelineDefinitionRepository, PipelineDefinitionRepositoryError,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use common::{validate_pipeline_definition, PipelineDefinition};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct PipelineDefinitionState {
    pub repository: Arc<dyn PipelineDefinitionRepository>,
    pub data_source_repository: Arc<dyn DataSourceRepository>,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({"error": message.into()}))).into_response()
}
fn repo_error(value: PipelineDefinitionRepositoryError) -> Response {
    match value {
        PipelineDefinitionRepositoryError::NotFound(id) => {
            error(StatusCode::NOT_FOUND, format!("no pipeline definition with id {id}"))
        }
        PipelineDefinitionRepositoryError::Backend(message) => {
            error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}
#[allow(clippy::result_large_err)]
fn validate(value: &PipelineDefinition) -> Result<(), Response> {
    validate_pipeline_definition(value).map_err(|message| error(StatusCode::BAD_REQUEST, message))
}
async fn require_data_source(
    state: &PipelineDefinitionState,
    value: &PipelineDefinition,
) -> Result<(), Response> {
    match state.data_source_repository.get(value.tenant_id, value.data_source_id).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(error(
            StatusCode::BAD_REQUEST,
            "data_source_id must reference a data source in the same tenant",
        )),
        Err(error_value) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, error_value.to_string())),
    }
}

pub async fn create_pipeline_definition(
    State(state): State<PipelineDefinitionState>,
    headers: HeaderMap,
    Json(mut value): Json<PipelineDefinition>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, value.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    if let Err(response) = validate(&value) {
        return response;
    }
    if let Err(response) = require_data_source(&state, &value).await {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    value.id = Uuid::new_v4();
    value.version = 1;
    let now = chrono::Utc::now();
    value.created_at = now;
    value.updated_at = now;
    match state.repository.create(value, &actor).await {
        Ok(value) => (StatusCode::CREATED, Json(value)).into_response(),
        Err(value) => repo_error(value),
    }
}
pub async fn list_pipeline_definitions(
    State(state): State<PipelineDefinitionState>,
    headers: HeaderMap,
) -> Response {
    match tenant_id_from_headers(&headers) {
        Ok(tenant_id) => match state.repository.list(tenant_id).await {
            Ok(values) => Json(values).into_response(),
            Err(value) => repo_error(value),
        },
        Err((status, message)) => error(status, message),
    }
}
pub async fn get_pipeline_definition(
    State(state): State<PipelineDefinitionState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    match tenant_id_from_headers(&headers) {
        Ok(tenant_id) => match state.repository.get(tenant_id, id).await {
            Ok(Some(value)) => Json(value).into_response(),
            Ok(None) => {
                error(StatusCode::NOT_FOUND, format!("no pipeline definition with id {id}"))
            }
            Err(value) => repo_error(value),
        },
        Err((status, message)) => error(status, message),
    }
}
pub async fn update_pipeline_definition(
    State(state): State<PipelineDefinitionState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut value): Json<PipelineDefinition>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, value.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    if let Err(response) = validate(&value) {
        return response;
    }
    if let Err(response) = require_data_source(&state, &value).await {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    value.id = id;
    match state.repository.update(value, &actor).await {
        Ok(value) => Json(value).into_response(),
        Err(value) => repo_error(value),
    }
}
pub async fn delete_pipeline_definition(
    State(state): State<PipelineDefinitionState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    let actor = match username_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    match state.repository.delete(tenant_id, id, &actor).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(value) => repo_error(value),
    }
}
