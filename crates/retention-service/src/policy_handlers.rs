#[path = "policy_handlers_test.rs"]
#[cfg(test)]
mod policy_handlers_test;

use crate::retention_policy::{RetentionPolicy, RetentionPolicyRepositoryError};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
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
    if let Some(response) = tenant_mismatch(&headers, policy.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    match state.policy_repository.create(policy).await {
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
    if let Some(response) = tenant_mismatch(&headers, policy.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    policy.id = id;
    match state.policy_repository.update(policy).await {
        Ok(updated) => Json(updated).into_response(),
        Err(e) => policy_error_response(e),
    }
}

pub async fn get_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
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
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.audit_reader.list_for_entity(tenant_id, entity_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
