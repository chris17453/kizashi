use common::connector::Connector;
use connector_graph_mail::GraphMailConnector;
use connector_runtime::{run_poll_cycle, HttpIngestionClient};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let tenant_id: Uuid = std::env::var("TENANT_ID")
        .expect("TENANT_ID must be set")
        .parse()
        .expect("TENANT_ID must be a UUID");
    let connector_id = std::env::var("CONNECTOR_ID").unwrap_or_else(|_| "graph-mail".to_string());
    let entra_tenant_id = std::env::var("ENTRA_TENANT_ID").expect("ENTRA_TENANT_ID must be set");
    let client_id = std::env::var("ENTRA_CLIENT_ID").expect("ENTRA_CLIENT_ID must be set");
    let client_secret =
        std::env::var("ENTRA_CLIENT_SECRET").expect("ENTRA_CLIENT_SECRET must be set");
    let user_id = std::env::var("GRAPH_MAIL_USER_ID").expect("GRAPH_MAIL_USER_ID must be set");
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");

    let token_url =
        format!("https://login.microsoftonline.com/{entra_tenant_id}/oauth2/v2.0/token");
    let connector = GraphMailConnector::new(
        connector_id,
        reqwest::Client::new(),
        "https://graph.microsoft.com/v1.0",
        token_url,
        client_id,
        client_secret,
        user_id,
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
