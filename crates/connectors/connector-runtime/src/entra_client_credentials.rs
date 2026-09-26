#[path = "entra_client_credentials_test.rs"]
#[cfg(test)]
mod entra_client_credentials_test;

use common::execute_oauth2_request;
use oauth2::basic::BasicClient;
use oauth2::{ClientId, ClientSecret, Scope, TokenResponse, TokenUrl};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EntraAuthError {
    #[error("invalid Entra app registration configuration: {0}")]
    Config(String),
    #[error("client-credentials token request failed: {0}")]
    TokenRequest(String),
}

/// The Entra ID app-only (client-credentials) flow ADR-0003 specifies for every
/// Entra-backed connector (`graph-mail`, `graph-teams`, `fabric`) — each tenant's own app
/// registration and secret, never a platform-wide service principal against a customer's
/// Azure tenant (ADR-0003's tenant-isolation rationale).
///
/// `http_client` is caller-provided rather than built internally (ADR-0025): every connector
/// already builds its own outbound `reqwest::Client` via `build_outbound_client`, optionally
/// proxied through Egress Gateway (ADR-0021) — reusing that same client for the token request
/// means the call to Entra's token endpoint is audited/allowlisted exactly like the connector's
/// data-plane calls, instead of silently bypassing Egress Gateway via oauth2's own internal
/// default client (`oauth2::reqwest::async_http_client`), which is what this function used to
/// do and is the gap this closes.
pub async fn fetch_access_token(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scope: &str,
    http_client: reqwest::Client,
) -> Result<String, EntraAuthError> {
    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
        oauth2::AuthUrl::new(token_url.to_string())
            .map_err(|e| EntraAuthError::Config(e.to_string()))?,
        Some(
            TokenUrl::new(token_url.to_string())
                .map_err(|e| EntraAuthError::Config(e.to_string()))?,
        ),
    );

    let token = client
        .exchange_client_credentials()
        .add_scope(Scope::new(scope.to_string()))
        .request_async(|request| execute_oauth2_request(&http_client, request))
        .await
        .map_err(|e| EntraAuthError::TokenRequest(e.to_string()))?;

    Ok(token.access_token().secret().clone())
}
