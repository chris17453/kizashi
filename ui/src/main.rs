use kizashi_ui::{
    build_router, AppState, HttpAnalysisConfigClient, HttpApiKeysClient, HttpAuditLogClient,
    HttpAuthClient, HttpBacklogClient, HttpEgressAllowlistClient, HttpEventsClient,
    HttpExecutionClient, HttpHealthClient, HttpIngestionStatsClient,
    HttpNormalizationMappingsClient, HttpRetentionPoliciesClient, HttpSavedSearchQueriesClient,
    HttpSensorsClient, HttpTriggersClient, HttpUsersClient, InMemorySessionStore,
};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let auth_service_url = std::env::var("AUTH_SERVICE_URL").expect("AUTH_SERVICE_URL must be set");
    let query_gateway_url =
        std::env::var("QUERY_GATEWAY_URL").expect("QUERY_GATEWAY_URL must be set");
    let config_admin_service_url =
        std::env::var("CONFIG_ADMIN_SERVICE_URL").expect("CONFIG_ADMIN_SERVICE_URL must be set");
    let observability_url =
        std::env::var("OBSERVABILITY_URL").expect("OBSERVABILITY_URL must be set");
    let ingestion_service_url =
        std::env::var("INGESTION_SERVICE_URL").expect("INGESTION_SERVICE_URL must be set");
    let ingestion_gateway_public_url = std::env::var("INGESTION_GATEWAY_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string());
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let action_executor_url =
        std::env::var("ACTION_EXECUTOR_URL").expect("ACTION_EXECUTOR_URL must be set");
    let retention_service_url =
        std::env::var("RETENTION_SERVICE_URL").expect("RETENTION_SERVICE_URL must be set");
    let egress_gateway_url =
        std::env::var("EGRESS_GATEWAY_URL").expect("EGRESS_GATEWAY_URL must be set");
    let trigger_engine_url =
        std::env::var("TRIGGER_ENGINE_URL").expect("TRIGGER_ENGINE_URL must be set");

    let client = reqwest::Client::new();
    let state = AppState {
        session_store: Arc::new(InMemorySessionStore::default()),
        auth_client: Arc::new(HttpAuthClient::new(client.clone(), auth_service_url.clone())),
        events_client: Arc::new(HttpEventsClient::new(client.clone(), query_gateway_url)),
        triggers_client: Arc::new(HttpTriggersClient::new(
            client.clone(),
            config_admin_service_url.clone(),
            trigger_engine_url,
        )),
        health_client: Arc::new(HttpHealthClient::new(client.clone(), observability_url.clone())),
        sensors_client: Arc::new(HttpSensorsClient::new(
            client.clone(),
            config_admin_service_url.clone(),
        )),
        api_keys_client: Arc::new(HttpApiKeysClient::new(client.clone(), ingestion_gateway_url)),
        backlog_client: Arc::new(HttpBacklogClient::new(client.clone(), observability_url)),
        stats_client: Arc::new(HttpIngestionStatsClient::new(
            client.clone(),
            ingestion_service_url,
        )),
        execution_client: Arc::new(HttpExecutionClient::new(client.clone(), action_executor_url)),
        analysis_config_client: Arc::new(HttpAnalysisConfigClient::new(
            client.clone(),
            config_admin_service_url.clone(),
        )),
        normalization_mappings_client: Arc::new(HttpNormalizationMappingsClient::new(
            client.clone(),
            config_admin_service_url.clone(),
        )),
        retention_policies_client: Arc::new(HttpRetentionPoliciesClient::new(
            client.clone(),
            retention_service_url.clone(),
        )),
        egress_allowlist_client: Arc::new(HttpEgressAllowlistClient::new(
            client.clone(),
            egress_gateway_url,
        )),
        config_audit_log_client: Arc::new(HttpAuditLogClient::new(
            client.clone(),
            config_admin_service_url.clone(),
        )),
        retention_audit_log_client: Arc::new(HttpAuditLogClient::new(
            client.clone(),
            retention_service_url,
        )),
        auth_audit_log_client: Arc::new(HttpAuditLogClient::new(
            client.clone(),
            auth_service_url.clone(),
        )),
        users_client: Arc::new(HttpUsersClient::new(client.clone(), auth_service_url)),
        saved_search_queries_client: Arc::new(HttpSavedSearchQueriesClient::new(
            client,
            config_admin_service_url,
        )),
        ingestion_gateway_public_url,
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "kizashi-ui listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
