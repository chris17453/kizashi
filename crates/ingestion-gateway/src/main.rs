use ingestion_gateway::{
    build_router, GatewayState, PostgresApiKeyStore, RateLimiter, SystemClock,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let ingestion_service_url =
        std::env::var("INGESTION_SERVICE_URL").expect("INGESTION_SERVICE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let rate_limit_per_minute: u32 =
        std::env::var("RATE_LIMIT_PER_MINUTE").ok().and_then(|v| v.parse().ok()).unwrap_or(600);

    let pool = common::connect_with_schema(&database_url, "ingestion_gateway")
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
        api_key_store: Arc::new(PostgresApiKeyStore::new(pool)),
        rate_limiter: Arc::new(RateLimiter::new(
            rate_limit_per_minute,
            Duration::from_secs(60),
            Box::new(SystemClock),
        )),
        http_client: reqwest::Client::new(),
        ingestion_service_url,
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "ingestion-gateway listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
