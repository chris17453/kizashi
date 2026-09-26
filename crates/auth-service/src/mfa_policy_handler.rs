use crate::local_login_handler::AuthState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct MfaPolicyResponse {
    pub required: bool,
}

#[derive(Deserialize)]
pub struct UpdateMfaPolicyRequest {
    pub required: bool,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

#[allow(clippy::result_large_err)]
fn tenant_id(headers: &HeaderMap) -> Result<Uuid, Response> {
    let Some(raw) = headers.get("x-tenant-id").and_then(|v| v.to_str().ok()) else {
        return Err(error_response(StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"));
    };
    Uuid::parse_str(raw).map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid tenant id"))
}

fn require_admin(headers: &HeaderMap) -> Option<Response> {
    let Some(raw) = headers.get("x-role").and_then(|v| v.to_str().ok()) else {
        return Some(error_response(StatusCode::UNAUTHORIZED, "missing X-Role header"));
    };
    match raw.parse::<Role>() {
        Ok(role) if role.at_least(Role::Admin) => None,
        Ok(_) => Some(error_response(StatusCode::FORBIDDEN, "admin role required")),
        Err(_) => Some(error_response(StatusCode::BAD_REQUEST, "invalid role")),
    }
}

#[allow(clippy::result_large_err)]
fn actor(headers: &HeaderMap) -> Result<String, Response> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

pub async fn get_mfa_policy(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    let tenant_id = match tenant_id(&headers) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match state.tenant_repository.mfa_required(tenant_id).await {
        Ok(required) => Json(MfaPolicyResponse { required }).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn put_mfa_policy(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<UpdateMfaPolicyRequest>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    let tenant_id = match tenant_id(&headers) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let actor = match actor(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match state.tenant_repository.set_mfa_required(tenant_id, req.required, &actor).await {
        Ok(()) => Json(MfaPolicyResponse { required: req.required }).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
