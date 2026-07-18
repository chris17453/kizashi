use dashboard_api::{build_router, ClickHouseEventQueryRepository, DashboardState};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let clickhouse_url = std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let state = DashboardState {
        event_query_repository: Arc::new(ClickHouseEventQueryRepository::new(
            reqwest::Client::new(),
            format!("{clickhouse_url}/"),
        )),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "dashboard-api listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
