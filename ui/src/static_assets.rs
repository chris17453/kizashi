#[path = "static_assets_test.rs"]
#[cfg(test)]
mod static_assets_test;

use axum::http::header;
use axum::response::{IntoResponse, Response};

const CHARTS_JS: &str = include_str!("../static/charts.js");

/// GET /static/charts.js — the vendored, dependency-free chart renderer (ADR-0015). Baked into
/// the binary via `include_str!` rather than served off disk: one less runtime dependency (no
/// static-file-serving middleware, no path traversal surface) for a single small file.
pub async fn get_charts_js() -> Response {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], CHARTS_JS).into_response()
}
