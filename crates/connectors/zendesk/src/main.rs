use common::connector::Connector;
use connector_runtime::{run_poll_cycle, HttpIngestionClient};
use connector_zendesk::ZendeskConnector;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let tenant_id: Uuid = std::env::var("TENANT_ID")
        .expect("TENANT_ID must be set")
        .parse()
        .expect("TENANT_ID must be a UUID");
    let connector_id = std::env::var("CONNECTOR_ID").unwrap_or_else(|_| "zendesk".to_string());
    let subdomain = std::env::var("ZENDESK_SUBDOMAIN").expect("ZENDESK_SUBDOMAIN must be set");
    let email = std::env::var("ZENDESK_EMAIL").expect("ZENDESK_EMAIL must be set");
    let api_token = std::env::var("ZENDESK_API_TOKEN").expect("ZENDESK_API_TOKEN must be set");
    let start_time: i64 = std::env::var("ZENDESK_START_TIME")
        .expect("ZENDESK_START_TIME must be set")
        .parse()
        .expect("ZENDESK_START_TIME must be a Unix timestamp");
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");

    let base_url = format!("https://{subdomain}.zendesk.com");
    let connector = ZendeskConnector::new(
        connector_id,
        reqwest::Client::new(),
        base_url,
        email,
        api_token,
        start_time,
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
