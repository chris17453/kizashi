#[path = "egress_client_test.rs"]
#[cfg(test)]
mod egress_client_test;

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EgressClientError {
    #[error("failed to configure egress proxy: {0}")]
    InvalidProxyUrl(String),
}

/// Builds the `reqwest::Client` a connector uses for its *outbound* calls to the external
/// source it polls (Zendesk/Graph/Fabric/customer SQL) — never the inbound
/// `HttpIngestionClient` calling back into Kizashi's own `ingestion-gateway`, which is a
/// different trust boundary entirely and must not be proxied through Egress Gateway.
///
/// When `egress_proxy_url` is `None` (the default — adoption is opt-in, ADR-0021), returns a
/// plain client, identical to today's behavior. When set, routes every request through
/// Egress Gateway with `Proxy-Authorization: Basic base64(tenant_id:connector_id)` — exactly
/// the identity contract `egress-gateway::parse_proxy_authorization` expects, so
/// `reqwest::Proxy::basic_auth` is the only client-side change needed to adopt it.
pub fn build_outbound_client(
    egress_proxy_url: Option<&str>,
    tenant_id: Uuid,
    connector_id: &str,
) -> Result<reqwest::Client, EgressClientError> {
    let Some(proxy_url) = egress_proxy_url else {
        return reqwest::Client::builder()
            .build()
            .map_err(|e| EgressClientError::InvalidProxyUrl(e.to_string()));
    };

    let proxy = reqwest::Proxy::all(proxy_url)
        .map_err(|e| EgressClientError::InvalidProxyUrl(e.to_string()))?
        .basic_auth(&tenant_id.to_string(), connector_id);

    reqwest::Client::builder()
        .proxy(proxy)
        .build()
        .map_err(|e| EgressClientError::InvalidProxyUrl(e.to_string()))
}
