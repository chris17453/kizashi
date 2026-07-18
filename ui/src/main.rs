use kizashi_ui::{
    build_router, AppState, HttpAuthClient, HttpEventsClient, HttpHealthClient, HttpTriggersClient,
    InMemorySessionStore,
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

    let client = reqwest::Client::new();
    let state = AppState {
        session_store: Arc::new(InMemorySessionStore::default()),
        auth_client: Arc::new(HttpAuthClient::new(client.clone(), auth_service_url)),
        events_client: Arc::new(HttpEventsClient::new(client.clone(), query_gateway_url)),
        triggers_client: Arc::new(HttpTriggersClient::new(
            client.clone(),
            config_admin_service_url,
        )),
        health_client: Arc::new(HttpHealthClient::new(client, observability_url)),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "kizashi-ui listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
