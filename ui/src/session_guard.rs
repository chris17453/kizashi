#[path = "session_guard_test.rs"]
#[cfg(test)]
mod session_guard_test;

use crate::{Session, SessionStore, SESSION_COOKIE_NAME};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};

/// Extracts a single named cookie's value from the `Cookie` header — a manual parse is simple
/// enough not to need a cookie-jar crate dependency (ADR-0014 keeps this UI's dependency
/// footprint small). Shared by the session cookie itself and the short-lived flow-bridging
/// cookies (OIDC's `kizashi_oidc_flow`, MFA's `kizashi_mfa_challenge`/`kizashi_mfa_username`,
/// ADR-0051).
pub fn session_cookie_value_named(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(|v| v.to_string()))
}

pub fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    session_cookie_value_named(headers, SESSION_COOKIE_NAME)
}

/// Every authenticated page's entry point: resolve the session or redirect to `/login`.
/// Returned as an `Err(Response)` so handlers can use `?`-style early return.
pub async fn require_session(
    session_store: &dyn SessionStore,
    headers: &HeaderMap,
) -> Result<Session, Response> {
    let session_id = session_cookie_value(headers);
    let session = match session_id {
        Some(id) => session_store.get(&id).await,
        None => None,
    };
    session.ok_or_else(|| Redirect::to("/login").into_response())
}
