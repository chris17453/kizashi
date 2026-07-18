#[path = "root_handler_test.rs"]
#[cfg(test)]
mod root_handler_test;

use axum::response::{IntoResponse, Redirect, Response};

/// GET / — otherwise unrouted, so `http://<host>` on its own 404s (found the hard way: it's
/// the exact URL every human types first). Redirects to `/overview`, which itself bounces an
/// unauthenticated visitor to `/login` via `require_session` — this handler doesn't need to
/// know about sessions at all.
pub async fn get_root() -> Response {
    Redirect::to("/overview").into_response()
}
