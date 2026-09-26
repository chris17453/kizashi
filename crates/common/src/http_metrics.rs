use axum::{
    extract::{Extension, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

#[derive(Clone, Default)]
pub struct HttpMetrics {
    requests: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    latency_micros: Arc<AtomicU64>,
}

impl HttpMetrics {
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
            self.latency_micros.load(Ordering::Relaxed),
        )
    }
}

pub async fn record_request(
    Extension(metrics): Extension<Arc<HttpMetrics>>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let response = next.run(request).await;
    metrics.requests.fetch_add(1, Ordering::Relaxed);
    if response.status().is_server_error() {
        metrics.errors.fetch_add(1, Ordering::Relaxed);
    }
    metrics.latency_micros.fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
    response
}

pub async fn metrics_handler(Extension(metrics): Extension<Arc<HttpMetrics>>) -> impl IntoResponse {
    let (requests, errors, latency_micros) = metrics.snapshot();
    let body = format!(
        "kizashi_http_requests_total {requests}\n\
kizashi_http_errors_total {errors}\n\
kizashi_http_request_duration_seconds_sum {:.6}\n\
kizashi_http_request_duration_seconds_count {requests}\n",
        latency_micros as f64 / 1_000_000.0
    );
    (StatusCode::OK, [("content-type", "text/plain; version=0.0.4")], body)
}

/// Adds the shared request metrics endpoint and middleware to a service router. The router is
/// expected to have its application state resolved with `with_state` already, which keeps this
/// helper usable across services with unrelated state types.
pub fn instrument_router(router: Router, metrics: Arc<HttpMetrics>) -> Router {
    router
        .route("/metrics", get(metrics_handler))
        .layer(axum::middleware::from_fn(record_request))
        .layer(Extension(metrics))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Extension, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn records_requests_errors_and_latency() {
        let metrics = Arc::new(HttpMetrics::default());
        let app = Router::new()
            .route("/ok", get(|| async { StatusCode::OK }))
            .route("/fail", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/metrics", get(metrics_handler))
            .layer(axum::middleware::from_fn(record_request))
            .layer(Extension(metrics.clone()));

        for path in ["/ok", "/fail", "/metrics"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let expected =
                if path == "/fail" { StatusCode::INTERNAL_SERVER_ERROR } else { StatusCode::OK };
            assert_eq!(response.status(), expected);
        }
        let (requests, errors, latency) = metrics.snapshot();
        assert_eq!(requests, 3);
        assert_eq!(errors, 1);
        assert!(latency > 0);
    }
}
