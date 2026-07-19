#[path = "entra_client_credentials_test.rs"]
#[cfg(test)]
mod entra_client_credentials_test;

use oauth2::basic::BasicClient;
use oauth2::reqwest::Error as OAuth2ReqwestError;
use oauth2::{ClientId, ClientSecret, HttpRequest, HttpResponse, Scope, TokenResponse, TokenUrl};
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
        .request_async(|request| execute_via(&http_client, request))
        .await
        .map_err(|e| EntraAuthError::TokenRequest(e.to_string()))?;

    Ok(token.access_token().secret().clone())
}

/// Mirrors `oauth2::reqwest::async_http_client`'s request/response translation, but against a
/// caller-supplied client instead of building a fresh default one internally. `oauth2` v4 and
/// `reqwest` v0.12 pin different major versions of the `http` crate (0.2 vs 1.x respectively),
/// so method/status/headers need an explicit bytes-level round trip between them rather than a
/// direct type reuse.
async fn execute_via(
    client: &reqwest::Client,
    request: HttpRequest,
) -> Result<HttpResponse, OAuth2ReqwestError<reqwest::Error>> {
    let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(|e| OAuth2ReqwestError::Other(e.to_string()))?;
    let mut request_builder = client.request(method, request.url.as_str()).body(request.body);
    for (name, value) in &request.headers {
        request_builder = request_builder.header(name.as_str(), value.as_bytes());
    }
    let built_request = request_builder.build().map_err(OAuth2ReqwestError::Reqwest)?;

    let response = client.execute(built_request).await.map_err(OAuth2ReqwestError::Reqwest)?;

    let status_code = oauth2::http::StatusCode::from_u16(response.status().as_u16())
        .map_err(|e| OAuth2ReqwestError::Other(e.to_string()))?;
    let mut headers = oauth2::http::HeaderMap::new();
    for (name, value) in response.headers() {
        let name = oauth2::http::HeaderName::from_bytes(name.as_str().as_bytes())
            .map_err(|e| OAuth2ReqwestError::Other(e.to_string()))?;
        let value = oauth2::http::HeaderValue::from_bytes(value.as_bytes())
            .map_err(|e| OAuth2ReqwestError::Other(e.to_string()))?;
        headers.insert(name, value);
    }
    let body = response.bytes().await.map_err(OAuth2ReqwestError::Reqwest)?.to_vec();

    Ok(HttpResponse { status_code, headers, body })
}
