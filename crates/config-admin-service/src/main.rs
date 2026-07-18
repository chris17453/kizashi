use config_admin_service::{
    build_router, AdminState, AgentState, PostgresAgentRepository, PostgresAuditLogReader,
    PostgresNormalizationMappingRepository, PostgresTriggerDefinitionRepository,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
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

    let state = AdminState {
        trigger_repository: Arc::new(PostgresTriggerDefinitionRepository::new(pool.clone())),
        mapping_repository: Arc::new(PostgresNormalizationMappingRepository::new(pool.clone())),
        audit_reader: Arc::new(PostgresAuditLogReader::new(pool.clone())),
    };
    let agent_state = AgentState { agent_repository: Arc::new(PostgresAgentRepository::new(pool)) };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "config-admin-service listening");
    axum::serve(listener, build_router(state, agent_state)).await.expect("server error");
}
