use common::connector::Connector;
use connector_graph_teams::GraphTeamsConnector;
use connector_runtime::{build_outbound_client, run_poll_cycle, HttpIngestionClient};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let tenant_id: Uuid = std::env::var("TENANT_ID")
        .expect("TENANT_ID must be set")
        .parse()
        .expect("TENANT_ID must be a UUID");
    let connector_id = std::env::var("CONNECTOR_ID").unwrap_or_else(|_| "graph-teams".to_string());
    let entra_tenant_id = std::env::var("ENTRA_TENANT_ID").expect("ENTRA_TENANT_ID must be set");
    let client_id = std::env::var("ENTRA_CLIENT_ID").expect("ENTRA_CLIENT_ID must be set");
    let client_secret =
        std::env::var("ENTRA_CLIENT_SECRET").expect("ENTRA_CLIENT_SECRET must be set");
    let team_id = std::env::var("GRAPH_TEAMS_TEAM_ID").expect("GRAPH_TEAMS_TEAM_ID must be set");
    let channel_id =
        std::env::var("GRAPH_TEAMS_CHANNEL_ID").expect("GRAPH_TEAMS_CHANNEL_ID must be set");
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");

    // ADR-0021: opt-in — set EGRESS_PROXY_URL to route this connector's outbound Graph API
    // calls through Egress Gateway's audit log/allowlist; unset means today's exact behavior.
    let egress_proxy_url = std::env::var("EGRESS_PROXY_URL").ok();
    let outbound_client =
        build_outbound_client(egress_proxy_url.as_deref(), tenant_id, &connector_id)
            .expect("failed to configure outbound HTTP client");

    let token_url =
        format!("https://login.microsoftonline.com/{entra_tenant_id}/oauth2/v2.0/token");
    let connector = GraphTeamsConnector::new(
        connector_id,
        outbound_client,
        "https://graph.microsoft.com/v1.0",
        token_url,
        client_id,
        client_secret,
        team_id,
        channel_id,
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
