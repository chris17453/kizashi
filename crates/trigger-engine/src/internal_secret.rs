#[path = "internal_secret_test.rs"]
#[cfg(test)]
mod internal_secret_test;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Shared-secret gate for trigger-engine's `/v1/triggers/*` routes (ADR-0009 pattern, same as
/// query-gateway's `/internal/tokens` and retention-service's `/v1/sweep`+`/v1/reimport`, see
/// `crates/retention-service/src/ops_handlers.rs`). Before this, any network caller able to
/// reach trigger-engine's published port could supply a plausible `X-Tenant-Id` and be trusted
/// as if it were Action Executor or the Console UI — there was no verification the caller was a
/// legitimate internal service at all. Applied as a single router-wide middleware layer (not
/// copy-pasted per handler) so no future handler on this router can be added without it.
/// `/healthz` is mounted on a *separate* router in `main.rs` (`health_router().merge(...)`), so
/// it is never wrapped by this layer and keeps working with the zero-header Docker healthcheck.
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
