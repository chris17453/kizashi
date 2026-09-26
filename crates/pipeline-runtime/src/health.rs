#[cfg(test)]
#[path = "health_test.rs"]
mod health_test;

use axum::http::StatusCode;

pub async fn healthz() -> StatusCode {
    StatusCode::OK
}
