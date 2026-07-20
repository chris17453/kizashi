#[path = "password_change_handler_test.rs"]
#[cfg(test)]
mod password_change_handler_test;

use crate::local_login_handler::AuthState;
use crate::password::{hash_password, verify_password};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<uuid::Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    uuid::Uuid::parse_str(raw)
        .map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

#[derive(serde::Deserialize)]
pub struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

/// POST /v1/auth/local/password — self-service password change (ADR-0057, closing a gap
/// ADR-0052 explicitly flagged: previously the only way to change a password at all was an
/// admin deleting and recreating the account). Requires re-entering the current password, same
/// reasoning as `post_mfa_disable` -- a hijacked but still-logged-in session shouldn't be able
/// to silently take over an account by locking the real owner out via a password change alone.
/// The new password goes through the same `validate_password_strength` check `create_user` uses,
/// so self-service can't bypass the policy admin-created accounts are held to.
pub async fn post_change_password(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    let user = match state.local_user_repository.find_by_username(tenant_id, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "no such user"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    if !verify_password(&req.current_password, &user.password_hash) {
        return error_response(StatusCode::UNAUTHORIZED, "current password is incorrect");
    }

    if let Err(e) =
        crate::password_policy::validate_password_strength(&req.new_password, &user.username)
    {
        return error_response(StatusCode::BAD_REQUEST, e.to_string());
    }

    let new_hash = match hash_password(&req.new_password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!(error = %e, "failed to hash password");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "failed to hash password");
        }
    };

    match state.local_user_repository.update_password(user.id, &new_hash).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
