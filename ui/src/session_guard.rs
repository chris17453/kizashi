#[path = "session_guard_test.rs"]
#[cfg(test)]
mod session_guard_test;

use crate::{Session, SessionStore, SESSION_COOKIE_NAME};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};

/// Extracts the session id from the `Cookie` header — a single-cookie parse is simple enough
/// not to need a cookie-jar crate dependency (ADR-0014 keeps this UI's dependency footprint
/// small).
pub fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').map(str::trim).find_map(|pair| {
        pair.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")).map(|v| v.to_string())
    })
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
