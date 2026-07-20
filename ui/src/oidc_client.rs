#[path = "oidc_client_test.rs"]
#[cfg(test)]
pub(crate) mod oidc_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum OidcClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("unknown OIDC provider")]
    UnknownProvider,
    #[error("unknown workspace")]
    UnknownWorkspace,
    #[error("oidc exchange failed: {0}")]
    ExchangeFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OidcAuthorization {
    pub authorization_url: String,
    pub csrf_token: String,
    pub code_verifier: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OidcSession {
    pub bearer_token: String,
    pub tenant_id: Uuid,
    pub role: Role,
    /// The identity the IdP asserted (email, falling back to subject) — always present on a
    /// real OIDC callback response, `None` only if a non-conforming server omits it.
    pub username: Option<String>,
}

/// Console UI's client for Auth Service's unified OIDC endpoints (ADR-0009) — the
/// authorize/callback exchange itself lives in Auth Service; this only drives it and, on
/// success, hands back what Console UI needs to mint its own `HttpOnly` session cookie
/// (ADR-0014), same division of responsibility as `AuthClient::local_login`.
#[async_trait]
pub trait OidcClient: Send + Sync {
    async fn authorize(&self, provider: &str) -> Result<OidcAuthorization, OidcClientError>;

    async fn callback(
        &self,
        provider: &str,
        code: &str,
        code_verifier: &str,
        tenant_name: &str,
    ) -> Result<OidcSession, OidcClientError>;
}

pub struct HttpOidcClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpOidcClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

#[derive(serde::Deserialize)]
struct AuthorizeResponse {
    authorization_url: String,
    csrf_token: String,
    code_verifier: String,
}

#[derive(serde::Deserialize)]
struct CallbackResponse {
    token: String,
    tenant_id: Uuid,
    role: Role,
    #[serde(default)]
    username: Option<String>,
}

#[async_trait]
impl OidcClient for HttpOidcClient {
    async fn authorize(&self, provider: &str) -> Result<OidcAuthorization, OidcClientError> {
        let response = self
            .client
            .get(format!("{}/v1/auth/oidc/{provider}/authorize", self.auth_service_url))
            .send()
            .await
            .map_err(|e| OidcClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(OidcClientError::UnknownProvider);
        }
        if !response.status().is_success() {
            return Err(OidcClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }

        let body: AuthorizeResponse =
            response.json().await.map_err(|e| OidcClientError::Unreachable(e.to_string()))?;
        Ok(OidcAuthorization {
            authorization_url: body.authorization_url,
            csrf_token: body.csrf_token,
            code_verifier: body.code_verifier,
        })
    }

    async fn callback(
        &self,
        provider: &str,
        code: &str,
        code_verifier: &str,
        tenant_name: &str,
    ) -> Result<OidcSession, OidcClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/oidc/{provider}/callback", self.auth_service_url))
            .json(&serde_json::json!({
                "code": code, "code_verifier": code_verifier, "tenant_name": tenant_name
            }))
            .send()
            .await
            .map_err(|e| OidcClientError::Unreachable(e.to_string()))?;

        match response.status() {
            reqwest::StatusCode::NOT_FOUND => return Err(OidcClientError::UnknownProvider),
            reqwest::StatusCode::BAD_REQUEST => return Err(OidcClientError::UnknownWorkspace),
            status if status.is_success() => {}
            status => {
                return Err(OidcClientError::ExchangeFailed(format!("unexpected status {status}")))
            }
        }

        let body: CallbackResponse =
            response.json().await.map_err(|e| OidcClientError::Unreachable(e.to_string()))?;
        Ok(OidcSession {
            bearer_token: body.token,
            tenant_id: body.tenant_id,
            role: body.role,
            username: body.username,
        })
    }
}
