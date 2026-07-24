use config_admin_service::{
    build_router, AdminState, AnalysisConfigState, ApiKeyEncryptor,
    PostgresAnalysisConfigRepository, PostgresAuditLogReader,
    PostgresEventTypeDefinitionRepository, PostgresNormalizationMappingRepository,
    PostgresReportRunRepository, PostgresSavedSearchQueryRepository, PostgresSensorRepository,
    PostgresTriggerDefinitionRepository, RabbitMqAnalysisConfigPublisher, RabbitMqMappingPublisher,
    RabbitMqSensorPublisher, RabbitMqTriggerPublisher, SavedSearchQueryState, SensorState,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
    let config_encryption_key =
        std::env::var("CONFIG_ENCRYPTION_KEY").expect("CONFIG_ENCRYPTION_KEY must be set");
    let api_key_encryptor = Arc::new(
        ApiKeyEncryptor::from_base64(&config_encryption_key)
            .expect("CONFIG_ENCRYPTION_KEY must be a base64-encoded 32-byte key"),
    );

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
    let sensor_publish_channel = connection.create_channel().await.expect("failed to open channel");
    let sensor_publisher = RabbitMqSensorPublisher::new(sensor_publish_channel)
        .await
        .expect("failed to declare sensor.changed exchange");
    let mapping_publish_channel =
        connection.create_channel().await.expect("failed to open channel");
    let mapping_publisher = RabbitMqMappingPublisher::new(mapping_publish_channel)
        .await
        .expect("failed to declare mapping.changed exchange");

    let state = AdminState {
        trigger_repository: Arc::new(PostgresTriggerDefinitionRepository::new(pool.clone())),
        mapping_repository: Arc::new(PostgresNormalizationMappingRepository::new(pool.clone())),
        audit_reader: Arc::new(PostgresAuditLogReader::new(pool.clone())),
        trigger_publisher: Arc::new(trigger_publisher),
        mapping_publisher: Arc::new(mapping_publisher),
        event_type_repository: Some(Arc::new(PostgresEventTypeDefinitionRepository::new(
            pool.clone(),
        ))),
        report_run_repository: Some(Arc::new(PostgresReportRunRepository::new(pool.clone()))),
    };
    let sensor_state = SensorState {
        sensor_repository: Arc::new(PostgresSensorRepository::new(pool.clone())),
        sensor_publisher: Arc::new(sensor_publisher),
    };
    let analysis_config_state = AnalysisConfigState {
        repository: Arc::new(PostgresAnalysisConfigRepository::new(
            pool.clone(),
            api_key_encryptor,
        )),
        publisher: Arc::new(analysis_config_publisher),
    };
    let saved_search_query_state = SavedSearchQueryState {
        saved_search_query_repository: Arc::new(PostgresSavedSearchQueryRepository::new(pool)),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "config-admin-service listening");
    axum::serve(
        listener,
        build_router(
            state,
            sensor_state,
            analysis_config_state,
            saved_search_query_state,
            internal_secret,
        ),
    )
    .await
    .expect("server error");
}
