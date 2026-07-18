use ingestion_service::{
    build_router, IngestState, PostgresRawRecordRepository, RabbitMqEventPublisher,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = sqlx::PgPool::connect(&database_url).await.expect("failed to connect to postgres");
    sqlx::migrate!("./migrations").run(&pool).await.expect("failed to run migrations");

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let channel = connection.create_channel().await.expect("failed to open rabbitmq channel");

    let state = IngestState {
        repository: Arc::new(PostgresRawRecordRepository::new(pool)),
        publisher: Arc::new(
            RabbitMqEventPublisher::new(channel).await.expect("failed to declare exchange"),
        ),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "ingestion-service listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
