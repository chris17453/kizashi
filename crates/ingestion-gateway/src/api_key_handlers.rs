#[path = "api_key_handlers_test.rs"]
#[cfg(test)]
mod api_key_handlers_test;

use crate::ingest_proxy_handler::{GatewayErrorBody, GatewayState};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Role;
use uuid::Uuid;

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(GatewayErrorBody { error: message.into() })).into_response()
}

/// Every handler here trusts `X-Tenant-Id` the same way config-admin-service's handlers do —
/// this is a Console-UI-to-service call, not the agent-facing `/v1/ingest` path (which
/// authenticates via `X-Api-Key` instead, since that's what it exists to issue).
fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

/// RBAC v1 follow-up (ADR-0016): API key creation/revocation is a write path, gated the same
/// way config-admin-service's/retention-service's writes are — `X-Role` at least `Operator`.
fn role_from_headers(headers: &HeaderMap) -> Result<Role, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Role header"))?;
    raw.parse().map_err(|_| (StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
}

/// Identifies the real human/operator performing a write, distinct from `tenant_id` (which
/// identifies the tenant being acted on, not who acted on it). Required for every handler that
/// writes an audit row (CLAUDE.md §5) — the audit log is useless for its compliance purpose if
/// `actor` can't be traced back to a person, since `tenant_id` is already a separate column on
/// every audit row. Same wire contract as auth-service/config-admin-service/retention-service's
/// `username_from_headers`.
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

#[derive(serde::Deserialize)]
pub struct CreateApiKeyRequest {
    pub label: String,
}

#[derive(serde::Serialize)]
pub struct CreatedApiKeyResponse {
    pub id: Uuid,
    pub label: String,
    /// The plaintext key — present only in this response, the one and only time it's ever
    /// available. Every other read (`list_api_keys`) returns summaries with no key material.
    pub api_key: String,
}

/// POST /v1/api-keys — issues a new key for the tenant, shown once.
pub async fn create_api_key(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(request): Json<CreateApiKeyRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.api_key_store.create(tenant_id, &request.label, &username).await {
        Ok((summary, plaintext)) => (
            StatusCode::CREATED,
            Json(CreatedApiKeyResponse {
                id: summary.id,
                label: summary.label,
                api_key: plaintext,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "api key creation failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

/// GET /v1/api-keys — lists the tenant's keys (no hash, no plaintext — just what an operator
/// needs to decide what to revoke).
pub async fn list_api_keys(State(state): State<GatewayState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.api_key_store.list(tenant_id).await {
        Ok(keys) => Json(keys).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "api key list failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

/// DELETE /v1/api-keys/:id — revokes a key. Idempotent: revoking an unknown or already-revoked
/// key still returns 204, matching `ApiKeyStore::revoke`'s no-op-not-error contract.
pub async fn revoke_api_key(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.api_key_store.revoke(tenant_id, id, &username).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "api key revocation failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

/// GET /v1/api-keys/:id/audit-log — the audit trail CLAUDE.md §5 requires for every
/// admin/config entity, same shape as config-admin-service's `get_audit_log`.
pub async fn get_api_key_audit_log(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.audit_reader.list_for_entity(tenant_id, id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "api key audit log lookup failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}
