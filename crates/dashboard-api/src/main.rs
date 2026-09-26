use axum::{middleware, routing::get, Extension};
use dashboard_api::{build_router, ClickHouseEventQueryRepository, DashboardState};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let clickhouse_url = std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let repository =
        ClickHouseEventQueryRepository::new(reqwest::Client::new(), format!("{clickhouse_url}/"));
    if let Err(error) = repository.ensure_schema().await {
        tracing::warn!(%error, "event status history schema could not be ensured");
    }
    let state = DashboardState { event_query_repository: Arc::new(repository) };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "dashboard-api listening");
    let metrics = Arc::new(common::HttpMetrics::default());
    let app = build_router(state)
        .route("/metrics", get(common::metrics_handler))
        .layer(middleware::from_fn(common::record_request))
        .layer(Extension(metrics));
    axum::serve(listener, app).await.expect("server error");
}
