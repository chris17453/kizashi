use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryServiceHealthChecker {
    pub statuses: Mutex<HashMap<String, Status>>,
}

#[async_trait]
impl ServiceHealthChecker for InMemoryServiceHealthChecker {
    async fn check(&self, endpoint: &ServiceEndpoint) -> Status {
        *self.statuses.lock().unwrap().get(&endpoint.name).unwrap_or(&Status::Down)
    }
}

fn endpoint(name: &str) -> ServiceEndpoint {
    ServiceEndpoint { name: name.to_string(), url: format!("http://{name}") }
}

#[tokio::test]
async fn platform_is_up_when_every_service_is_up() {
    let checker = InMemoryServiceHealthChecker::default();
    checker.statuses.lock().unwrap().insert("a".to_string(), Status::Up);
    checker.statuses.lock().unwrap().insert("b".to_string(), Status::Up);
    let registry = vec![endpoint("a"), endpoint("b")];

    let health = check_platform_health(&checker, &registry).await;

    assert_eq!(health.status, Status::Up);
    assert!(health.services.iter().all(|s| s.status == Status::Up));
}

#[tokio::test]
async fn platform_is_down_when_any_one_service_is_down() {
    let checker = InMemoryServiceHealthChecker::default();
    checker.statuses.lock().unwrap().insert("a".to_string(), Status::Up);
    checker.statuses.lock().unwrap().insert("b".to_string(), Status::Down);
    let registry = vec![endpoint("a"), endpoint("b")];

    let health = check_platform_health(&checker, &registry).await;

    assert_eq!(health.status, Status::Down);
    let b = health.services.iter().find(|s| s.name == "b").unwrap();
    assert_eq!(b.status, Status::Down);
}

#[tokio::test]
async fn an_unregistered_service_defaults_to_down() {
    let checker = InMemoryServiceHealthChecker::default();
    let registry = vec![endpoint("unregistered")];

    let health = check_platform_health(&checker, &registry).await;

    assert_eq!(health.status, Status::Down);
}

async fn spawn_stub_server(status: axum::http::StatusCode) -> String {
    async fn ok_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::OK
    }
    async fn error_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    }
    let app = if status.is_success() {
        axum::Router::new().route("/healthz", axum::routing::get(ok_handler))
    } else {
        axum::Router::new().route("/healthz", axum::routing::get(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_checker_reports_up_for_a_real_healthy_server() {
    let url = spawn_stub_server(axum::http::StatusCode::OK).await;
    let checker = HttpServiceHealthChecker::new(reqwest::Client::new());
    let status = checker.check(&ServiceEndpoint { name: "svc".to_string(), url }).await;
    assert_eq!(status, Status::Up);
}

#[tokio::test]
async fn http_checker_reports_down_for_a_real_unhealthy_server() {
    let url = spawn_stub_server(axum::http::StatusCode::SERVICE_UNAVAILABLE).await;
    let checker = HttpServiceHealthChecker::new(reqwest::Client::new());
    let status = checker.check(&ServiceEndpoint { name: "svc".to_string(), url }).await;
    assert_eq!(status, Status::Down);
}

#[tokio::test]
async fn http_checker_reports_down_when_server_is_unreachable() {
    let checker = HttpServiceHealthChecker::new(reqwest::Client::new());
    let status = checker
        .check(&ServiceEndpoint { name: "svc".to_string(), url: "http://127.0.0.1:1".to_string() })
        .await;
    assert_eq!(status, Status::Down);
}

#[tokio::test]
async fn http_checker_reads_request_metrics_when_instrumented() {
    async fn metrics() -> &'static str {
        "kizashi_http_requests_total 12\nkizashi_http_errors_total 2\nkizashi_http_request_duration_seconds_sum 1.5\nkizashi_http_request_duration_seconds_count 10\n"
    }
    let app = axum::Router::new().route("/metrics", axum::routing::get(metrics));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let checker = HttpServiceHealthChecker::new(reqwest::Client::new());
    let metrics = checker
        .metrics(&ServiceEndpoint { name: "svc".to_string(), url: format!("http://{addr}") })
        .await
        .unwrap();
    assert_eq!(metrics.name, "svc");
    assert_eq!(metrics.requests, 12);
    assert_eq!(metrics.errors, 2);
    assert_eq!(metrics.latency_seconds_count, 10);
}
