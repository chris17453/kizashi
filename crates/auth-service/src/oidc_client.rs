#[path = "oidc_client_test.rs"]
#[cfg(test)]
pub(crate) mod oidc_client_test;

use async_trait::async_trait;
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OidcError {
    #[error("invalid provider configuration: {0}")]
    Config(String),
    #[error("code exchange failed: {0}")]
    Exchange(String),
    #[error("userinfo request failed: {0}")]
    Userinfo(String),
}

#[derive(Debug, Clone)]
pub struct OidcProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub redirect_url: String,
}

#[derive(Debug, PartialEq)]
pub struct AuthorizationRequest {
    pub authorization_url: String,
    pub csrf_token: String,
    /// Returned to the caller because Auth Service has no session/cookie layer yet (ADR-0009)
    /// — whatever is driving the browser redirect (Console UI, once built) is responsible for
    /// holding this and posting it back to the callback endpoint alongside the auth code.
    pub code_verifier: String,
}

#[derive(Debug, PartialEq)]
pub struct OidcUserInfo {
    pub subject: String,
    pub email: Option<String>,
}

/// A tenant-configured OIDC identity provider. The same client works for Entra ID and any
/// other OIDC-compliant "generic OAuth" provider (ADR-0009) — only the configured endpoints
/// and credentials differ.
#[async_trait]
pub trait OidcClient: Send + Sync {
    fn authorization_request(&self) -> Result<AuthorizationRequest, OidcError>;
    async fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<String, OidcError>;
    async fn fetch_userinfo(&self, access_token: &str) -> Result<OidcUserInfo, OidcError>;
}

pub struct StandardOidcClient {
    inner: BasicClient,
    userinfo_url: String,
    http_client: reqwest::Client,
    scopes: Vec<String>,
}

impl StandardOidcClient {
    pub fn new(config: OidcProviderConfig) -> Result<Self, OidcError> {
        let inner = BasicClient::new(
            ClientId::new(config.client_id),
            Some(ClientSecret::new(config.client_secret)),
            AuthUrl::new(config.auth_url).map_err(|e| OidcError::Config(e.to_string()))?,
            Some(TokenUrl::new(config.token_url).map_err(|e| OidcError::Config(e.to_string()))?),
        )
        .set_redirect_uri(
            RedirectUrl::new(config.redirect_url).map_err(|e| OidcError::Config(e.to_string()))?,
        );

        Ok(Self {
            inner,
            userinfo_url: config.userinfo_url,
            http_client: reqwest::Client::new(),
            scopes: vec!["openid".to_string(), "email".to_string(), "profile".to_string()],
        })
    }
}

#[async_trait]
impl OidcClient for StandardOidcClient {
    fn authorization_request(&self) -> Result<AuthorizationRequest, OidcError> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request =
            self.inner.authorize_url(CsrfToken::new_random).set_pkce_challenge(pkce_challenge);
        for scope in &self.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let (url, csrf_token) = request.url();
        Ok(AuthorizationRequest {
            authorization_url: url.to_string(),
            csrf_token: csrf_token.secret().clone(),
            code_verifier: pkce_verifier.secret().clone(),
        })
    }

    async fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<String, OidcError> {
        let result = self
            .inner
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_pkce_verifier(PkceCodeVerifier::new(code_verifier.to_string()))
            .request_async(async_http_client)
            .await
            .map_err(|e| OidcError::Exchange(e.to_string()))?;
        Ok(result.access_token().secret().clone())
    }

    async fn fetch_userinfo(&self, access_token: &str) -> Result<OidcUserInfo, OidcError> {
        let response = self
            .http_client
            .get(&self.userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| OidcError::Userinfo(e.to_string()))?;

        if !response.status().is_success() {
            return Err(OidcError::Userinfo(format!("HTTP {}", response.status().as_u16())));
        }

        #[derive(serde::Deserialize)]
        struct RawUserInfo {
            sub: String,
            email: Option<String>,
        }
        let raw: RawUserInfo =
            response.json().await.map_err(|e| OidcError::Userinfo(e.to_string()))?;
        Ok(OidcUserInfo { subject: raw.sub, email: raw.email })
    }
}
