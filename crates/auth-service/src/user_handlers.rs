#[path = "user_handlers_test.rs"]
#[cfg(test)]
pub(crate) mod user_handlers_test;

#[path = "user_handlers_audit_actor_test.rs"]
#[cfg(test)]
mod user_handlers_audit_actor_test;

#[path = "recent_audit_log_handler_test.rs"]
#[cfg(test)]
mod recent_audit_log_handler_test;

#[path = "session_revoked_audit_handler_test.rs"]
#[cfg(test)]
mod session_revoked_audit_handler_test;

#[path = "audit_log_error_hygiene_test.rs"]
#[cfg(test)]
mod audit_log_error_hygiene_test;

use crate::local_login_handler::AuthState;
use crate::local_user_repository::{LocalUser, LocalUserRepositoryError};
use crate::password::hash_password;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use common::Role;
use uuid::Uuid;

/// Default and max page size for `GET /v1/audit-log` — small enough to keep a compliance
/// reviewer's page snappy, generous enough that a max-out request still needs only a handful of
/// pages to page back through a busy tenant's history.
const DEFAULT_RECENT_AUDIT_LOG_LIMIT: u32 = 50;
const MAX_RECENT_AUDIT_LOG_LIMIT: u32 = 200;

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

/// The real identity of the caller (as opposed to `tenant_id_from_headers`, which only
/// identifies *which tenant*), so audit log rows (`AuditLogEntry.actor`) record who performed
/// the action rather than the tenant_id — which is already a separate column on every row and
/// tells an auditor nothing about "who" (CLAUDE.md §5).
fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))
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

/// Duplicate-key conflicts get a clean, specific message since that's a real, expected outcome
/// a caller can act on ("pick a different username"). Every other `Backend` error is logged in
/// full server-side and replaced with a generic message before it reaches the client -- the raw
/// string can be anything from a SQL syntax error to a connection-pool timeout, and was
/// previously passed straight through as the HTTP body, visible verbatim to any Admin using the
/// Console UI. Internal failure detail belongs in logs, not in a response an operator sees.
fn user_error_response(e: LocalUserRepositoryError) -> Response {
    match e {
        LocalUserRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no user with id {id}"))
        }
        LocalUserRepositoryError::Backend(msg) if msg.contains("duplicate key") => {
            error_response(StatusCode::CONFLICT, "username already exists in this tenant")
        }
        LocalUserRepositoryError::Backend(msg) => {
            tracing::error!(error = %msg, "user repository backend error");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
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
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if let Err(e) = crate::password_policy::validate_password_strength(&req.password, &req.username)
    {
        return error_response(StatusCode::BAD_REQUEST, e.to_string());
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
        mfa_secret: None,
        mfa_enabled: false,
    };

    match state.local_user_repository.create(user, &actor).await {
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
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
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
    match state.local_user_repository.update_role(tenant_id, id, req.role, &actor).await {
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
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
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
    match state.local_user_repository.delete(tenant_id, id, &actor).await {
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
        Err(e) => {
            tracing::error!(error = %e, "audit log lookup failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

#[derive(serde::Deserialize)]
pub struct RecentAuditLogQuery {
    pub limit: Option<u32>,
    pub before: Option<DateTime<Utc>>,
}

/// `GET /v1/audit-log` — the general, chronological "show me every admin action in the last N
/// days" trail (SOC2/ISO27001-style expectation) that `get_user_audit_log` above can't answer
/// since it requires already knowing an entity's UUID. Read-only, so no `require_admin` gate:
/// same convention as the entity-scoped endpoint, any authenticated tenant member may view it.
pub async fn get_recent_audit_log(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Query(query): Query<RecentAuditLogQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let limit =
        query.limit.unwrap_or(DEFAULT_RECENT_AUDIT_LOG_LIMIT).min(MAX_RECENT_AUDIT_LOG_LIMIT);
    match state.audit_log_reader.list_recent(tenant_id, limit as i64, query.before).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "recent audit log lookup failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

/// GET /v1/auth/local/password-policy — the policy's live parameters (ADR-0056), so the Console
/// UI's compliance report describes what's actually enforced rather than a hardcoded copy that
/// could silently drift. Not tenant-scoped or sensitive (it's the rule, not anyone's data), so
/// no `X-Tenant-Id`/`X-Role` check -- same "public within the internal-secret gate" bar as
/// nothing else on this router, actually, but there's no per-tenant variation to leak either.
pub async fn get_password_policy() -> Response {
    Json(crate::password_policy::summary()).into_response()
}

#[derive(serde::Deserialize)]
pub struct SessionRevokedRequest {
    pub session_id: Uuid,
    pub revoked_username: String,
}

/// POST /v1/audit-log/session-revoked — Console UI's session store is purely in-memory
/// (ADR-0014), so it has no durable trail of its own; this records the fact of a revocation
/// here instead, under `entity_type = "session"`, closing the gap a tenth UI audit pass found
/// (every other destructive admin action writes an audit entry, session revoke wrote none).
/// `Admin`-only, matching `/security/sessions`' own access bar in the Console UI.
pub async fn post_session_revoked_audit(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<SessionRevokedRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    match state
        .session_audit_writer
        .record_revocation(tenant_id, &actor, req.session_id, &req.revoked_username)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session revocation audit write failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}
