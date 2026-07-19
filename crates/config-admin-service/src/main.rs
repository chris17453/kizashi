use config_admin_service::{
    build_router, AdminState, AgentState, AnalysisConfigState, PostgresAgentRepository,
    PostgresAnalysisConfigRepository, PostgresAuditLogReader,
    PostgresNormalizationMappingRepository, PostgresTriggerDefinitionRepository,
    RabbitMqAgentPublisher, RabbitMqAnalysisConfigPublisher, RabbitMqTriggerPublisher,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = common::connect_with_schema(&database_url, "config_admin_service")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let publish_channel = connection.create_channel().await.expect("failed to open channel");
    let trigger_publisher = RabbitMqTriggerPublisher::new(publish_channel)
        .await
        .expect("failed to declare trigger.changed exchange");
    let analysis_config_publish_channel =
        connection.create_channel().await.expect("failed to open channel");
    let analysis_config_publisher =
        RabbitMqAnalysisConfigPublisher::new(analysis_config_publish_channel)
            .await
            .expect("failed to declare analysis_config.changed exchange");
    let agent_publish_channel = connection.create_channel().await.expect("failed to open channel");
    let agent_publisher = RabbitMqAgentPublisher::new(agent_publish_channel)
        .await
        .expect("failed to declare agent.changed exchange");

    let state = AdminState {
        trigger_repository: Arc::new(PostgresTriggerDefinitionRepository::new(pool.clone())),
        mapping_repository: Arc::new(PostgresNormalizationMappingRepository::new(pool.clone())),
        audit_reader: Arc::new(PostgresAuditLogReader::new(pool.clone())),
        trigger_publisher: Arc::new(trigger_publisher),
    };
    let agent_state = AgentState {
        agent_repository: Arc::new(PostgresAgentRepository::new(pool.clone())),
        agent_publisher: Arc::new(agent_publisher),
    };
    let analysis_config_state = AnalysisConfigState {
        repository: Arc::new(PostgresAnalysisConfigRepository::new(pool)),
        publisher: Arc::new(analysis_config_publisher),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "config-admin-service listening");
    axum::serve(listener, build_router(state, agent_state, analysis_config_state))
        .await
        .expect("server error");
}
