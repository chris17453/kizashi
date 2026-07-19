use common::connector::Connector;
use connector_imap::ImapConnector;
use connector_runtime::{run_poll_cycle, HttpIngestionClient};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let tenant_id: Uuid = std::env::var("TENANT_ID")
        .expect("TENANT_ID must be set")
        .parse()
        .expect("TENANT_ID must be a UUID");
    let connector_id = std::env::var("CONNECTOR_ID").unwrap_or_else(|_| "imap".to_string());
    let host = std::env::var("IMAP_HOST").expect("IMAP_HOST must be set");
    let port: u16 = std::env::var("IMAP_PORT")
        .unwrap_or_else(|_| "993".to_string())
        .parse()
        .expect("IMAP_PORT must be a port number");
    let username = std::env::var("IMAP_USERNAME").expect("IMAP_USERNAME must be set");
    let password = std::env::var("IMAP_PASSWORD").expect("IMAP_PASSWORD must be set");
    let mailbox = std::env::var("IMAP_MAILBOX").unwrap_or_else(|_| "INBOX".to_string());
    let since_date: chrono::NaiveDate = std::env::var("IMAP_SINCE_DATE")
        .expect("IMAP_SINCE_DATE must be set (YYYY-MM-DD)")
        .parse()
        .expect("IMAP_SINCE_DATE must be a YYYY-MM-DD date");
    let use_tls: bool = std::env::var("IMAP_USE_TLS")
        .unwrap_or_else(|_| "true".to_string())
        .parse()
        .expect("IMAP_USE_TLS must be true or false");
    let since_uid: Option<u32> = std::env::var("IMAP_SINCE_UID").ok().and_then(|v| v.parse().ok());
    // Caps a single poll's fetch instead of pulling an entire backfill window in one shot —
    // this is what makes backfill "chunked, resumable, rate-limit-friendly" rather than one
    // giant burst (ADR-0034). 200 is a conservative default for a 5-minute default poll
    // interval; operators with a larger poll interval or higher ingestion rate limit can raise
    // it per-Agent.
    let max_records_per_poll: Option<usize> = Some(
        std::env::var("IMAP_MAX_RECORDS_PER_POLL").ok().and_then(|v| v.parse().ok()).unwrap_or(200),
    );
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");

    // Note: unlike the HTTP-based connectors, this connector's own source poll is a raw
    // TLS+IMAP TCP connection, not an HTTP request — it cannot be routed through Egress
    // Gateway's HTTP CONNECT tunnel the way EGRESS_PROXY_URL wires HTTP connectors (ADR-0021).
    // Proxying/auditing raw IMAP traffic is tracked as a follow-up, same known-gap treatment as
    // the fabric/sql connectors' non-HTTP protocols.
    let connector = ImapConnector::new(
        connector_id,
        host,
        port,
        username,
        password,
        mailbox,
        since_date,
        use_tls,
    )
    .with_since_uid(since_uid)
    .with_max_records_per_poll(max_records_per_poll);
    let ingestion_client =
        HttpIngestionClient::new(reqwest::Client::new(), ingestion_gateway_url, api_key);

    match run_poll_cycle(&connector, tenant_id, &ingestion_client).await {
        Ok(summary) => {
            tracing::info!(
                ?summary,
                connector_id = connector.connector_id(),
                "poll cycle complete"
            );
            // A machine-parseable marker on its own stdout line so the orchestrator
            // (DockerInvoker) can capture and persist it without needing structured logging —
            // see ADR-0034. Only emitted when this poll actually produced a checkpoint.
            if let Some(checkpoint) = &summary.checkpoint {
                println!("KIZASHI_CHECKPOINT={checkpoint}");
            }
        }
        Err(e) => {
            tracing::error!(error = %e, connector_id = connector.connector_id(), "poll cycle failed");
            std::process::exit(1);
        }
    }
}
