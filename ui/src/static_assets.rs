#[path = "static_assets_test.rs"]
#[cfg(test)]
mod static_assets_test;

use axum::http::header;
use axum::response::{IntoResponse, Response};

const CHARTS_JS: &str = include_str!("../static/charts.js");
const CONFIRM_DANGER_JS: &str = include_str!("../static/confirm-danger.js");

/// GET /static/charts.js — the vendored, dependency-free chart renderer (ADR-0015). Baked into
/// the binary via `include_str!` rather than served off disk: one less runtime dependency (no
/// static-file-serving middleware, no path traversal surface) for a single small file.
pub async fn get_charts_js() -> Response {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], CHARTS_JS).into_response()
}

/// GET /static/confirm-danger.js — confirms every destructive form submission before it fires
/// (ADR-0061), same embed-in-binary approach as `get_charts_js`.
pub async fn get_confirm_danger_js() -> Response {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], CONFIRM_DANGER_JS).into_response()
}
