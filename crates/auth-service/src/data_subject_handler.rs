#[path = "data_subject_handler_test.rs"]
#[cfg(test)]
mod data_subject_handler_test;

use crate::audit_log::AuditLogEntry;
use crate::local_login_handler::AuthState;
use crate::local_user_repository::LocalUser;
use crate::login_attempt_repository::LoginAttempt;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
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
            "role does not have permission to export data subject records",
        )),
        Err(_) => Some(error_response(StatusCode::BAD_REQUEST, "X-Role is not a recognized role")),
    }
}

#[derive(serde::Serialize)]
struct DataSubjectExport {
    user: LocalUser,
    audit_log: Vec<AuditLogEntry>,
    login_attempts: Vec<LoginAttempt>,
}

/// GET /v1/users/:id/data-subject-export — everything Kizashi itself directly attributes to one
/// local user account (ADR-0054): the account record, every admin action taken on it
/// (`auth_audit_log`, already tenant-scoped by `list_for_entity`), and every login/MFA attempt
/// recorded against its username. `Admin`-only, same bar as `/v1/users` (ADR-0016 follow-up).
///
/// Deliberately does not attempt to search ingested `RawRecord`/`Event` content for this
/// person -- there is no reliable, indexed identity field across arbitrary source-system payloads
/// (see ADR-0054), so that's out of scope for v1 rather than silently incomplete.
pub async fn get_data_subject_export(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let user = match state.local_user_repository.find_by_id(id).await {
        Ok(Some(user)) if user.tenant_id == tenant_id => user,
        Ok(_) => return error_response(StatusCode::NOT_FOUND, format!("no user with id {id}")),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let audit_log = match state.audit_log_reader.list_for_entity(tenant_id, id).await {
        Ok(entries) => entries,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let login_attempts = match state.login_attempt_repository.list_by_username(&user.username).await
    {
        Ok(attempts) => attempts,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    Json(DataSubjectExport { user, audit_log, login_attempts }).into_response()
}
