#[path = "user_handlers_test.rs"]
#[cfg(test)]
mod user_handlers_test;

use crate::local_login_handler::AuthState;
use crate::local_user_repository::{LocalUser, LocalUserRepositoryError};
use crate::password::hash_password;
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

fn role_from_headers(headers: &HeaderMap) -> Result<Role, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Role header"))?;
    raw.parse().map_err(|_| (StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
}

/// User management (ADR-0016 follow-up: the "assign role to another user" surface explicitly
/// deferred by RBAC v1) is reserved for `Admin` — a step above the `Operator` bar every other
/// write path uses, since granting/revoking access is more sensitive than editing config
/// entities.
fn require_admin(headers: &HeaderMap) -> Option<Response> {
    match role_from_headers(headers) {
        Ok(role) if role.at_least(Role::Admin) => None,
        Ok(_) => Some(error_response(
            StatusCode::FORBIDDEN,
            "role does not have permission to manage users",
        )),
        Err((status, msg)) => Some(error_response(status, msg)),
    }
}

/// Guards against leaving a tenant with no `Admin` able to manage users/roles — the one
/// self-inflicted lockout this service *can* detect and prevent without a user identity in the
/// session (ADR-0016's still-open limitation, see `audit_log.rs`), since it only needs to count
/// admins tenant-wide, not identify "self". Returns `true` when `target_id` is currently the
/// tenant's only `Admin`.
async fn is_sole_admin(
    state: &AuthState,
    tenant_id: Uuid,
    target_id: Uuid,
) -> Result<bool, Response> {
    let users = state.local_user_repository.list(tenant_id).await.map_err(user_error_response)?;
    let mut admins = users.iter().filter(|u| u.role == Role::Admin);
    match (admins.next(), admins.next()) {
        (Some(only), None) => Ok(only.id == target_id),
        _ => Ok(false),
    }
}

fn user_error_response(e: LocalUserRepositoryError) -> Response {
    match e {
        LocalUserRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no user with id {id}"))
        }
        LocalUserRepositoryError::Backend(msg) if msg.contains("duplicate key") => {
            error_response(StatusCode::CONFLICT, "username already exists in this tenant")
        }
        LocalUserRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: Role,
}

pub async fn create_user(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let password_hash = match hash_password(&req.password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!(error = %e, "failed to hash password");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "failed to hash password");
        }
    };

    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: req.username,
        password_hash,
        role: req.role,
    };

    match state.local_user_repository.create(user).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(e) => user_error_response(e),
    }
}

pub async fn list_users(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    match state.local_user_repository.list(tenant_id).await {
        Ok(users) => Json(users).into_response(),
        Err(e) => user_error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct UpdateUserRoleRequest {
    pub role: Role,
}

pub async fn update_user_role(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserRoleRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    if req.role != Role::Admin {
        match is_sole_admin(&state, tenant_id, id).await {
            Ok(true) => {
                return error_response(
                    StatusCode::CONFLICT,
                    "cannot demote the tenant's only Admin — promote another user first",
                );
            }
            Ok(false) => {}
            Err(response) => return response,
        }
    }
    match state
        .local_user_repository
        .update_role(tenant_id, id, req.role, &tenant_id.to_string())
        .await
    {
        Ok(updated) => Json(updated).into_response(),
        Err(e) => user_error_response(e),
    }
}

pub async fn delete_user(
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
    match is_sole_admin(&state, tenant_id, id).await {
        Ok(true) => {
            return error_response(
                StatusCode::CONFLICT,
                "cannot delete the tenant's only Admin — promote another user first",
            );
        }
        Ok(false) => {}
        Err(response) => return response,
    }
    match state.local_user_repository.delete(tenant_id, id, &tenant_id.to_string()).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => user_error_response(e),
    }
}

pub async fn get_user_audit_log(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.audit_log_reader.list_for_entity(tenant_id, id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
