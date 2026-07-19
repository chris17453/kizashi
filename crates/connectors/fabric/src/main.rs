use common::connector::Connector;
use connector_fabric::FabricConnector;
use connector_runtime::{build_outbound_client, run_poll_cycle, HttpIngestionClient};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let tenant_id: Uuid = std::env::var("TENANT_ID")
        .expect("TENANT_ID must be set")
        .parse()
        .expect("TENANT_ID must be a UUID");
    let connector_id = std::env::var("CONNECTOR_ID").unwrap_or_else(|_| "fabric".to_string());
    let entra_tenant_id = std::env::var("ENTRA_TENANT_ID").expect("ENTRA_TENANT_ID must be set");
    let client_id = std::env::var("ENTRA_CLIENT_ID").expect("ENTRA_CLIENT_ID must be set");
    let client_secret =
        std::env::var("ENTRA_CLIENT_SECRET").expect("ENTRA_CLIENT_SECRET must be set");
    let host = std::env::var("FABRIC_SQL_HOST").expect("FABRIC_SQL_HOST must be set");
    let port: u16 = std::env::var("FABRIC_SQL_PORT")
        .unwrap_or_else(|_| "1433".to_string())
        .parse()
        .expect("FABRIC_SQL_PORT must be a port number");
    let database = std::env::var("FABRIC_SQL_DATABASE").expect("FABRIC_SQL_DATABASE must be set");
    let query = std::env::var("FABRIC_SQL_QUERY").expect("FABRIC_SQL_QUERY must be set");
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");

    // ADR-0021/ADR-0025: fabric's SQL data path is TDS, not HTTP, so it has no outbound
    // reqwest::Client to proxy for the data-plane itself — but the Entra token fetch below IS
    // an HTTP call, so it gets the same opt-in EGRESS_PROXY_URL treatment as every other
    // connector's outbound HTTP calls.
    let egress_proxy_url = std::env::var("EGRESS_PROXY_URL").ok();
    let token_client = build_outbound_client(egress_proxy_url.as_deref(), tenant_id, &connector_id)
        .expect("failed to configure outbound HTTP client for the Entra token fetch");

    let token_url =
        format!("https://login.microsoftonline.com/{entra_tenant_id}/oauth2/v2.0/token");
    let connector = FabricConnector::new(
        connector_id,
        host,
        port,
        database,
        token_url,
        client_id,
        client_secret,
        query,
        false, // real Fabric always presents a valid certificate
        token_client,
    );
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
