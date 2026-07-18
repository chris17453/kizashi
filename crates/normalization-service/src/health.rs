#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

use axum::{routing::get, Router};

pub fn build_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

pub async fn healthz() -> &'static str {
    "ok"
}
