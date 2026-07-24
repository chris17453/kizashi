use crate::compliance_hold::{ComplianceHold, ComplianceHoldRepositoryError};
use crate::policy_handlers::{
    require_internal_secret, require_operator, tenant_id_from_headers, tenant_mismatch,
    username_from_headers,
};
use crate::retention_policy::DataClass;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateHoldRequest {
    pub tenant_id: Uuid,
    pub data_class: DataClass,
    pub reason: String,
}

fn hold_repo(
    state: &AppState,
) -> Result<&std::sync::Arc<dyn crate::compliance_hold::ComplianceHoldRepository>, Response> {
    state.hold_repository.as_ref().ok_or_else(|| {
        (StatusCode::NOT_IMPLEMENTED, "compliance hold registry unavailable").into_response()
    })
}

fn hold_error(error: ComplianceHoldRepositoryError) -> Response {
    match error {
        ComplianceHoldRepositoryError::NotFound(id) => {
            (StatusCode::NOT_FOUND, format!("no compliance hold with id {id}")).into_response()
        }
        ComplianceHoldRepositoryError::Backend(message) => {
            (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
        }
    }
}

pub async fn list_holds(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return (status, message).into_response(),
    };
    let repo = match hold_repo(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.list(tenant_id).await {
        Ok(holds) => Json(holds).into_response(),
        Err(error) => hold_error(error),
    }
}

pub async fn create_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateHoldRequest>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    if let Some(response) = tenant_mismatch(&headers, request.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(actor) => actor,
        Err((status, message)) => return (status, message).into_response(),
    };
    if request.reason.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "hold reason is required").into_response();
    }
    let hold = ComplianceHold {
        id: Uuid::new_v4(),
        tenant_id: request.tenant_id,
        data_class: request.data_class,
        reason: request.reason.trim().to_string(),
        active: true,
        created_by: actor.clone(),
        created_at: Utc::now(),
        released_at: None,
    };
    let repo = match hold_repo(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.create(hold, &actor).await {
        Ok(hold) => (StatusCode::CREATED, Json(hold)).into_response(),
        Err(error) => hold_error(error),
    }
}

pub async fn release_hold(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return (status, message).into_response(),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(actor) => actor,
        Err((status, message)) => return (status, message).into_response(),
    };
    let repo = match hold_repo(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.release(tenant_id, id, &actor).await {
        Ok(hold) => Json(hold).into_response(),
        Err(error) => hold_error(error),
    }
}
