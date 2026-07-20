#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

use crate::allowlist::AllowlistRepository;
use crate::allowlist_audit_log::AllowlistAuditLogReader;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use common::Role;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AdminState {
    pub allowlist_repository: Arc<dyn AllowlistRepository>,
    pub allowlist_audit_log_reader: Arc<dyn AllowlistAuditLogReader>,
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))
}

/// The real user identity behind an allowlist change, written to `allowlist_audit_log`'s
/// `actor` column — same trust boundary as `X-Tenant-Id`/`X-Role` (CLAUDE.md §5).
fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

/// RBAC v1 (ADR-0016): rejects the request unless the caller's role is at least `Operator` —
/// the same check every other config-mutating write endpoint in the platform runs
/// (config-admin-service's trigger/mapping/agent writes, retention-service's policy writes,
/// ingestion-gateway's API key writes). `PUT /v1/allowlist` controls a tenant's egress
/// SSRF/exfiltration containment boundary (ADR-0021) and had no role check at all until now —
/// closing that gap is why this exists here, matching `config_admin_service::require_operator`.
fn require_operator(headers: &HeaderMap) -> Option<Response> {
    let raw = match headers.get("x-role").and_then(|v| v.to_str().ok()) {
        Some(raw) => raw,
        None => return Some(error_response(StatusCode::UNAUTHORIZED, "missing X-Role header")),
    };
    let role: Role = match raw.parse() {
        Ok(role) => role,
        Err(_) => {
            return Some(error_response(StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
        }
    };
    if role.at_least(Role::Operator) {
        None
    } else {
        Some(error_response(
            StatusCode::FORBIDDEN,
            "role does not have permission to perform this action",
        ))
    }
}

/// GET /v1/allowlist — the calling tenant's configured domain allowlist, `[]` meaning "no
/// restriction configured" (ADR-0021).
async fn get_allowlist(State(state): State<AdminState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.allowlist_repository.get_domains(&tenant_id).await {
        Ok(domains) => Json(domains).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(serde::Deserialize)]
struct PutAllowlistBody {
    domains: Vec<String>,
}

/// PUT /v1/allowlist — replaces the calling tenant's domain allowlist wholesale (not a
/// per-domain add/remove API — the list is small and operator-managed, replace-the-whole-thing
/// matches how `AnalysisConfig`'s single prompt is edited).
async fn put_allowlist(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(body): Json<PutAllowlistBody>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(actor) => actor,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.allowlist_repository.set_domains(&tenant_id, body.domains.clone(), &actor).await {
        Ok(()) => Json(body.domains).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// GET /v1/audit-log/:entity_id — same generic shape config-admin-service/retention-service/
/// auth-service already expose, so the Console UI's existing `HttpAuditLogClient` can read this
/// back with no new client type (ADR-0097).
async fn get_audit_log(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(entity_id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let Ok(tenant_uuid) = Uuid::parse_str(&tenant_id) else {
        return error_response(StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID");
    };
    match state.allowlist_audit_log_reader.list_for_entity(tenant_uuid, entity_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub fn build_router(state: AdminState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/allowlist", get(get_allowlist).put(put_allowlist))
        .route("/v1/audit-log/:entity_id", get(get_audit_log))
        .with_state(state)
}
