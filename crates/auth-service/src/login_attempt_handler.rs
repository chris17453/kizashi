#[path = "login_attempt_handler_test.rs"]
#[cfg(test)]
mod login_attempt_handler_test;

use crate::local_login_handler::AuthState;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use common::Role;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

fn require_admin(headers: &HeaderMap) -> Option<Response> {
    let raw = match headers.get("x-role").and_then(|v| v.to_str().ok()) {
        Some(raw) => raw,
        None => return Some(error_response(StatusCode::UNAUTHORIZED, "missing X-Role header")),
    };
    match raw.parse::<Role>() {
        Ok(role) if role.at_least(Role::Admin) => None,
        Ok(_) => Some(error_response(
            StatusCode::FORBIDDEN,
            "role does not have permission to view login attempts",
        )),
        Err(_) => Some(error_response(StatusCode::BAD_REQUEST, "X-Role is not a recognized role")),
    }
}

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(serde::Deserialize)]
pub struct LoginAttemptQuery {
    limit: Option<u32>,
    before: Option<DateTime<Utc>>,
}

/// GET /v1/auth/local/login-attempts — every recent local-login and MFA-challenge attempt for
/// the tenant, successful or not (ADR-0053). `Admin`-only: this is security telemetry about the
/// whole tenant's access attempts, not a self-service view of one's own account, matching the
/// access bar already used for `/v1/users` (ADR-0016 follow-up) and Active Sessions.
pub async fn get_login_attempts(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Query(query): Query<LoginAttemptQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let limit = query.limit.map(|l| l as i64).unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    match state.login_attempt_repository.list_recent(tenant_id, limit, query.before).await {
        Ok(attempts) => Json(attempts).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
