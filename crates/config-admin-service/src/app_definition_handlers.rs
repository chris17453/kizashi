use crate::{
    app_definition_repository::{AppDefinitionRepository, AppDefinitionRepositoryError},
    handlers::{require_operator, tenant_id_from_headers, tenant_mismatch, username_from_headers},
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use common::{validate_app_definition, AppDefinition};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppDefinitionState {
    pub repository: Arc<dyn AppDefinitionRepository>,
}
fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({"error":message.into()}))).into_response()
}
fn repository_error(error_value: AppDefinitionRepositoryError) -> Response {
    match error_value {
        AppDefinitionRepositoryError::NotFound(id) => {
            error(StatusCode::NOT_FOUND, format!("no app definition with id {id}"))
        }
        AppDefinitionRepositoryError::Backend(message) => {
            error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}
pub async fn create_app_definition(
    State(state): State<AppDefinitionState>,
    headers: HeaderMap,
    Json(mut value): Json<AppDefinition>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, value.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    if let Err(message) = validate_app_definition(&value) {
        return error(StatusCode::BAD_REQUEST, message);
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
        Err(error_value) => repository_error(error_value),
    }
}
pub async fn list_app_definitions(
    State(state): State<AppDefinitionState>,
    headers: HeaderMap,
) -> Response {
    match tenant_id_from_headers(&headers) {
        Ok(tenant_id) => match state.repository.list(tenant_id).await {
            Ok(values) => Json(values).into_response(),
            Err(error_value) => repository_error(error_value),
        },
        Err((status, message)) => error(status, message),
    }
}
pub async fn get_app_definition(
    State(state): State<AppDefinitionState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    match tenant_id_from_headers(&headers) {
        Ok(tenant_id) => match state.repository.get(tenant_id, id).await {
            Ok(Some(value)) => Json(value).into_response(),
            Ok(None) => error(StatusCode::NOT_FOUND, format!("no app definition with id {id}")),
            Err(error_value) => repository_error(error_value),
        },
        Err((status, message)) => error(status, message),
    }
}
pub async fn update_app_definition(
    State(state): State<AppDefinitionState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut value): Json<AppDefinition>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, value.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    if let Err(message) = validate_app_definition(&value) {
        return error(StatusCode::BAD_REQUEST, message);
    }
    let actor = match username_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    value.id = id;
    match state.repository.update(value, &actor).await {
        Ok(value) => Json(value).into_response(),
        Err(error_value) => repository_error(error_value),
    }
}
