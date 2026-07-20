#[path = "policy_handlers_auth_test.rs"]
#[cfg(test)]
mod policy_handlers_auth_test;
#[path = "policy_handlers_test.rs"]
#[cfg(test)]
mod policy_handlers_test;

use crate::ops_handlers::has_valid_internal_secret;
use crate::retention_policy::{RetentionPolicy, RetentionPolicyRepositoryError};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use common::Role;
#[cfg(test)]
use std::sync::Arc;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Every handler trusts `X-Tenant-Id` as set by whatever gateway sits in front of this service
/// (spec §8), same convention as config-admin-service and dashboard-api.
fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

fn tenant_mismatch(headers: &HeaderMap, entity_tenant_id: Uuid) -> Option<Response> {
    match tenant_id_from_headers(headers) {
        Ok(tenant_id) if tenant_id == entity_tenant_id => None,
        Ok(_) => {
            Some(error_response(StatusCode::FORBIDDEN, "tenant_id does not match X-Tenant-Id"))
        }
        Err((status, msg)) => Some(error_response(status, msg)),
    }
}

/// RBAC v1 follow-up (ADR-0016): same `X-Role` trust boundary and `Operator`-minimum check as
/// config-admin-service's write handlers.
fn role_from_headers(headers: &HeaderMap) -> Result<Role, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Role header"))?;
    raw.parse().map_err(|_| (StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
}

/// The real user who performed the action, as set by whatever gateway sits in front of this
/// service — distinct from `X-Tenant-Id` (which identifies the tenant, not the person), so audit
/// log rows (CLAUDE.md §5) can actually answer "who did this," not just "which tenant." Same
/// convention as auth-service, config-admin-service, and ingestion-gateway.
fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

fn require_operator(headers: &HeaderMap) -> Option<Response> {
    match role_from_headers(headers) {
        Ok(role) if role.at_least(Role::Operator) => None,
        Ok(_) => Some(error_response(
            StatusCode::FORBIDDEN,
            "role does not have permission to perform this action",
        )),
        Err((status, msg)) => Some(error_response(status, msg)),
    }
}

/// Security audit finding: these routes trusted `X-Role`/`X-Tenant-Id`/`X-Username` at face
/// value with no verification the caller was actually the Console UI (docker-compose publishes
/// this service's port directly, so any network caller could `curl -H "X-Role: admin" ...`
/// straight at it). Gate every policy route behind the same `X-Internal-Secret` shared-secret
/// check `ops_handlers.rs` already uses for `/v1/sweep` and `/v1/reimport`, in addition to (not
/// instead of) the tenant/role/username checks below.
fn require_internal_secret(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    if has_valid_internal_secret(state, headers) {
        None
    } else {
        Some(error_response(StatusCode::UNAUTHORIZED, "invalid internal secret"))
    }
}

fn policy_error_response(e: RetentionPolicyRepositoryError) -> Response {
    match e {
        RetentionPolicyRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no retention policy with id {id}"))
        }
        RetentionPolicyRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

pub async fn create_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(policy): Json<RetentionPolicy>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    if let Some(response) = tenant_mismatch(&headers, policy.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.policy_repository.create(policy, &actor).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(e) => policy_error_response(e),
    }
}

pub async fn update_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut policy): Json<RetentionPolicy>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    if let Some(response) = tenant_mismatch(&headers, policy.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    policy.id = id;
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.policy_repository.update(policy, &actor).await {
        Ok(updated) => Json(updated).into_response(),
        Err(e) => policy_error_response(e),
    }
}

pub async fn delete_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.policy_repository.delete(tenant_id, id, &actor).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => policy_error_response(e),
    }
}

pub async fn get_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.policy_repository.get(tenant_id, id).await {
        Ok(Some(policy)) => Json(policy).into_response(),
        Ok(None) => {
            error_response(StatusCode::NOT_FOUND, format!("no retention policy with id {id}"))
        }
        Err(e) => policy_error_response(e),
    }
}

pub async fn list_policies(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.policy_repository.list(tenant_id).await {
        Ok(policies) => Json(policies).into_response(),
        Err(e) => policy_error_response(e),
    }
}

/// GET /v1/audit-log/:entity_id — the audit trail CLAUDE.md §5 requires for every admin/config
/// mutation, retention policy changes included (spec §8).
pub async fn get_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entity_id): Path<Uuid>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.audit_reader.list_for_entity(tenant_id, entity_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

const DEFAULT_RECENT_AUDIT_LOG_LIMIT: i64 = 50;
const MAX_RECENT_AUDIT_LOG_LIMIT: i64 = 200;

#[derive(serde::Deserialize)]
pub struct RecentAuditLogParams {
    limit: Option<u32>,
    before: Option<DateTime<Utc>>,
}

/// GET /v1/audit-log (no `entity_id` segment — axum disambiguates it from
/// `/v1/audit-log/:entity_id` fine) — the general, cross-entity chronological audit trail a
/// SOC2/ISO27001 auditor expects ("show me every admin action in the last N days"), as opposed
/// to `get_audit_log`'s single-entity history. Cursor-paginated via `before`; most-recent-first.
pub async fn get_recent_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<RecentAuditLogParams>,
) -> Response {
    if let Some(response) = require_internal_secret(&state, &headers) {
        return response;
    }
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let limit = params
        .limit
        .map(|l| l as i64)
        .unwrap_or(DEFAULT_RECENT_AUDIT_LOG_LIMIT)
        .min(MAX_RECENT_AUDIT_LOG_LIMIT);
    match state.audit_reader.list_recent(tenant_id, limit, params.before).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
