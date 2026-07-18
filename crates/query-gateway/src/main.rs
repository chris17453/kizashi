use query_gateway::{build_router, health_router, GatewayState, PostgresTokenStore};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let dashboard_api_url =
        std::env::var("DASHBOARD_API_URL").expect("DASHBOARD_API_URL must be set");
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = common::connect_with_schema(&database_url, "query_gateway")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let state = GatewayState {
        token_store: Arc::new(PostgresTokenStore::new(pool)),
        http_client: reqwest::Client::new(),
        dashboard_api_url,
        internal_secret,
    };

    let app = health_router().merge(build_router(state));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "query-gateway listening");
    axum::serve(listener, app).await.expect("server error");
}
