//! Router-level gate requiring `X-Internal-Secret` to match `INTERNAL_API_SECRET` on every
//! config-admin-service route except `/healthz`. Before this, `AdminState`/`SensorState`/
//! `AnalysisConfigState`/`SavedSearchQueryState` handlers trusted `X-Role`/`X-Tenant-Id`/
//! `X-Username` headers at face value with zero verification that the caller was actually the
//! Console UI — since docker-compose publishes this service's port directly, any network caller
//! could `curl -H "X-Role: admin" ...` straight at it and be trusted. Applied as ONE
//! `axum::middleware::from_fn_with_state` layer on the merged router (see `lib.rs::build_router`)
//! rather than copy-pasted into every handler, so no future handler can be added without it —
//! same shared-secret pattern as query-gateway's `/internal/tokens` and retention-service's
//! `/v1/sweep` + `/v1/reimport` (ADR-0009).

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
