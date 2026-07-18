use observability::{
    build_router, parse_registry, AppState, HttpServiceHealthChecker,
    RabbitMqManagementBacklogReader,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let registry = parse_registry(&std::env::var("SERVICE_REGISTRY").unwrap_or_default());
    let rabbitmq_management_url =
        std::env::var("RABBITMQ_MANAGEMENT_URL").expect("RABBITMQ_MANAGEMENT_URL must be set");
    let rabbitmq_vhost = std::env::var("RABBITMQ_VHOST").unwrap_or_else(|_| "/".to_string());

    let client = reqwest::Client::new();
    let state = AppState {
        health_checker: Arc::new(HttpServiceHealthChecker::new(client.clone())),
        registry: Arc::new(registry),
        backlog_reader: Arc::new(RabbitMqManagementBacklogReader::new(
            client,
            rabbitmq_management_url,
            rabbitmq_vhost,
        )),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "observability listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
