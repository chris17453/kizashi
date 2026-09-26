use incident_service::{
    build_router, run_correlation_consumer, IncidentState, PostgresAuditLogReader,
    PostgresIncidentRepository,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = common::connect_with_schema(&database_url, "incident_service")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let state = IncidentState {
        incident_repository: Arc::new(PostgresIncidentRepository::new(pool.clone())),
        audit_log_reader: Arc::new(PostgresAuditLogReader::new(pool)),
    };
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    tokio::spawn(run_correlation_consumer(rabbitmq_url, state.incident_repository.clone()));

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "incident-service listening");
    let metrics = Arc::new(common::HttpMetrics::default());
    axum::serve(listener, common::instrument_router(build_router(state), metrics))
        .await
        .expect("server error");
}
