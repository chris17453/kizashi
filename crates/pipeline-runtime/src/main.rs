use pipeline_runtime::{
    build_router, publish_once, HttpProjectionClient, PostgresExecutionRepository,
    PostgresOutboxRepository, PostgresWorkflowRepository, RuntimeState,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let ontology_service_url = std::env::var("ONTOLOGY_SERVICE_URL")
        .unwrap_or_else(|_| "http://ontology-service:8080".to_string());
    let pool = common::connect_with_schema(&database_url, "pipeline_runtime")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");
    let publisher_repository = Arc::new(PostgresOutboxRepository::new(pool.clone()));
    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let channel = connection.create_channel().await.expect("failed to open RabbitMQ channel");
    tokio::spawn(async move {
        loop {
            if let Err(error) = publish_once(publisher_repository.as_ref(), &channel).await {
                tracing::error!(%error, "pipeline outbox publish failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
    let metrics = Arc::new(common::HttpMetrics::default());
    let state = RuntimeState {
        repository: Arc::new(PostgresExecutionRepository::new(pool.clone())),
        projection_client: Some(Arc::new(HttpProjectionClient::new(
            reqwest::Client::new(),
            ontology_service_url,
            internal_secret.clone(),
        ))),
        workflow_repository: Arc::new(PostgresWorkflowRepository::new(pool)),
        internal_secret,
    };
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "pipeline-runtime listening");
    axum::serve(listener, common::instrument_router(build_router(state), metrics))
        .await
        .expect("server error");
}
