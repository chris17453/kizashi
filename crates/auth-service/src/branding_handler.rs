#[path = "branding_handler_test.rs"]
#[cfg(test)]
mod branding_handler_test;

use crate::local_login_handler::AuthState;
use crate::tenant_branding_repository::TenantBranding;
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

fn role_from_headers(headers: &HeaderMap) -> Result<Role, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Role header"))?;
    raw.parse().map_err(|_| (StatusCode::BAD_REQUEST, "X-Role is not a recognized role"))
}

fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw).map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

/// The Console UI renders `accent_color` into a `<style>` block on the login page
/// (unauthenticated, pre-login — every visitor who knows a workspace name sees it), so it must
/// be restricted to an actual CSS hex color, not free text — anything else is a CSS injection
/// vector (layout breakage at best, attribute-selector-based data exfiltration at worst; CSS
/// itself can't execute script, but that's not the same as "safe to embed unvalidated").
fn is_valid_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else { return false };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// GET /v1/tenants/:name/branding — deliberately unauthenticated: the one caller that needs
/// this (the Console UI's login page) hasn't authenticated anyone yet, and branding (a product
/// name, a logo, a color) isn't sensitive information — workspace *names* are already visible
/// in the URL a customer's operators share with each other.
pub async fn get_branding(State(state): State<AuthState>, Path(name): Path<String>) -> Response {
    match state.tenant_branding_repository.branding_for_name(&name).await {
        Ok(Some(branding)) => Json(branding).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "unknown workspace"),
        Err(e) => {
            tracing::error!(error = %e, "branding lookup failed");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "branding lookup failed")
        }
    }
}

/// GET /v1/tenants/id/:id/branding — same data as `get_branding`, keyed by id instead of name
/// for the authenticated Settings page, which only ever has a `tenant_id` from the session.
pub async fn get_branding_by_id(
    State(state): State<AuthState>,
    Path(tenant_id): Path<Uuid>,
) -> Response {
    match state.tenant_branding_repository.branding_for_id(tenant_id).await {
        Ok(Some(branding)) => Json(branding).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "unknown workspace"),
        Err(e) => {
            tracing::error!(error = %e, "branding lookup failed");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "branding lookup failed")
        }
    }
}

/// PUT /v1/tenants/:id/branding — admin-only (white-label settings are a workspace-wide
/// identity change, not a per-user preference), audit-logged with the real actor (ADR-0039).
pub async fn put_branding(
    State(state): State<AuthState>,
    Path(tenant_id): Path<Uuid>,
    headers: HeaderMap,
    Json(branding): Json<TenantBranding>,
) -> Response {
    let role = match role_from_headers(&headers) {
        Ok(role) => role,
        Err((status, message)) => return error_response(status, message),
    };
    if role != Role::Admin {
        return error_response(StatusCode::FORBIDDEN, "admin role required");
    }
    let actor = match username_from_headers(&headers) {
        Ok(actor) => actor,
        Err((status, message)) => return error_response(status, message),
    };
    // Without this, any Admin (of any tenant) could overwrite another tenant's branding just by
    // knowing/guessing its id — the path `:id` alone was never checked against the caller's own
    // tenant. `X-Tenant-Id` is the same header every other write handler in this codebase
    // trusts for this purpose.
    let caller_tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, message)) => return error_response(status, message),
    };
    if caller_tenant_id != tenant_id {
        return error_response(StatusCode::FORBIDDEN, "tenant_id does not match X-Tenant-Id");
    }
    if let Some(color) = &branding.accent_color {
        if !is_valid_hex_color(color) {
            return error_response(
                StatusCode::BAD_REQUEST,
                "accent_color must be a CSS hex color like #22d3ee",
            );
        }
    }

    match state.tenant_branding_repository.update_branding(tenant_id, branding, &actor).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "branding update failed");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "branding update failed")
        }
    }
}
