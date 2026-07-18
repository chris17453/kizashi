use common::connector::Connector;
use connector_runtime::{run_poll_cycle, HttpIngestionClient};
use connector_sql::SqlConnector;
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let tenant_id: Uuid = std::env::var("TENANT_ID")
        .expect("TENANT_ID must be set")
        .parse()
        .expect("TENANT_ID must be a UUID");
    let connector_id = std::env::var("CONNECTOR_ID").unwrap_or_else(|_| "sql".to_string());
    let source_database_url =
        std::env::var("SQL_SOURCE_DATABASE_URL").expect("SQL_SOURCE_DATABASE_URL must be set");
    let query = std::env::var("SQL_QUERY").expect("SQL_QUERY must be set");
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");

    let pool =
        PgPool::connect(&source_database_url).await.expect("failed to connect to source database");
    let connector = SqlConnector::new(connector_id, pool, query);
    let ingestion_client =
        HttpIngestionClient::new(reqwest::Client::new(), ingestion_gateway_url, api_key);

    match run_poll_cycle(&connector, tenant_id, &ingestion_client).await {
        Ok(summary) => {
            tracing::info!(
                ?summary,
                connector_id = connector.connector_id(),
                "poll cycle complete"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, connector_id = connector.connector_id(), "poll cycle failed");
            std::process::exit(1);
        }
    }
}
