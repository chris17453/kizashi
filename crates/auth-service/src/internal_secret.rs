//! Router-level gate requiring `X-Internal-Secret` to match `INTERNAL_API_SECRET` on every
//! auth-service route that trusts `X-Role`/`X-Tenant-Id`/`X-Username` at face value —
//! `PUT /v1/tenants/:id/branding` (`branding_handler::put_branding`) and the user-management
//! handlers in `user_handlers.rs`. Before this, those headers were trusted with zero
//! verification that the caller was actually the Console UI — since docker-compose publishes
//! this service's port directly, any network caller could `curl -H "X-Role: admin" ...` straight
//! at it and be trusted. Applied as ONE `axum::middleware::from_fn_with_state` layer on the
//! protected half of the merged router (see `lib.rs::build_router`) rather than copy-pasted into
//! every handler, so no future handler can be added to that half without it.
//!
//! Deliberately NOT applied to `/v1/auth/local/login`, `/v1/auth/oidc/:provider/authorize`,
//! `/v1/auth/oidc/:provider/callback`, or `GET /v1/tenants/:name/branding` — those are the
//! pre-session, browser-facing entry points a real end user's browser hits directly (or that
//! render before any session/role exists), so they never read the trust headers this gate
//! protects and gating them would break login entirely.
//!
//! Same shared-secret pattern as query-gateway's `/internal/tokens`, retention-service's
//! `/v1/sweep` + `/v1/reimport`, and config-admin-service's router-wide gate (ADR-0009).

#[path = "internal_secret_test.rs"]
#[cfg(test)]
mod internal_secret_test;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub async fn require_internal_secret(
    State(secret): State<String>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let provided = headers.get("x-internal-secret").and_then(|v| v.to_str().ok());
    if provided != Some(secret.as_str()) {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }
    next.run(request).await
}
