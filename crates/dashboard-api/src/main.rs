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
    axum::serve(listener, build_router(state)).await.expect("server error");
}
