#[path = "action_template_handlers_test.rs"]
#[cfg(test)]
mod action_template_handlers_test;

use crate::action_template_repository::{ActionTemplateRepository, ActionTemplateRepositoryError};
use crate::handlers::{
    require_operator, tenant_id_from_headers, tenant_mismatch, username_from_headers,
};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::{validate_action_template_config, ActionTemplate, ActionType};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ActionTemplateState {
    pub repository: Arc<dyn ActionTemplateRepository>,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({"error": message.into()}))).into_response()
}

fn repository_error(repository_error: ActionTemplateRepositoryError) -> Response {
    match repository_error {
        ActionTemplateRepositoryError::NotFound(id) => {
            error(StatusCode::NOT_FOUND, format!("no action template with id {id}"))
        }
        ActionTemplateRepositoryError::Backend(message) => {
            error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

#[allow(clippy::result_large_err)]
fn validate(template: &ActionTemplate) -> Result<(), Response> {
    if template.name.trim().is_empty() || template.name.len() > 160 {
        return Err(error(StatusCode::BAD_REQUEST, "name must be between 1 and 160 characters"));
    }
    if template.description.len() > 2000 {
        return Err(error(StatusCode::BAD_REQUEST, "description is too long"));
    }
    if !template.config.is_object() {
        return Err(error(StatusCode::BAD_REQUEST, "config must be a JSON object"));
    }
    if let Err(message) = validate_action_template_config(template.action_type, &template.config) {
        return Err(error(StatusCode::BAD_REQUEST, message));
    }
    Ok(())
}

pub async fn create_action_template(
    State(state): State<ActionTemplateState>,
    headers: HeaderMap,
    Json(mut template): Json<ActionTemplate>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, template.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    if let Err(response) = validate(&template) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    template.id = if template.id == Uuid::nil() { Uuid::new_v4() } else { template.id };
    template.version = 1;
    let now = chrono::Utc::now();
    template.created_at = now;
    template.updated_at = now;
    match state.repository.create(template, &actor).await {
        Ok(value) => (StatusCode::CREATED, Json(value)).into_response(),
        Err(error) => repository_error(error),
    }
}

pub async fn list_action_templates(
    State(state): State<ActionTemplateState>,
    headers: HeaderMap,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    match state.repository.list(tenant_id).await {
        Ok(values) => Json(values).into_response(),
        Err(error) => repository_error(error),
    }
}

pub async fn get_action_template(
    State(state): State<ActionTemplateState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    match state.repository.get(tenant_id, id).await {
        Ok(Some(value)) => Json(value).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, format!("no action template with id {id}")),
        Err(error) => repository_error(error),
    }
}

pub async fn update_action_template(
    State(state): State<ActionTemplateState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut template): Json<ActionTemplate>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, template.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    if let Err(response) = validate(&template) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(value) => value,
        Err((status, message)) => return error(status, message),
    };
    template.id = id;
    match state.repository.update(template, &actor).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => repository_error(error),
    }
}

pub async fn delete_action_template(
    State(state): State<ActionTemplateState>,
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
        Err(error) => repository_error(error),
    }
}

#[allow(dead_code)]
fn supported_action_type(action_type: ActionType) -> bool {
    matches!(
        action_type,
        ActionType::Email
            | ActionType::Webhook
            | ActionType::TeamsAlert
            | ActionType::CreateTicket
            | ActionType::Custom
            | ActionType::GeneratePdf
            | ActionType::GenerateXlsx
    )
}
