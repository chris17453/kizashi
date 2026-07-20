#[path = "mfa_client_test.rs"]
#[cfg(test)]
pub(crate) mod mfa_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MfaClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("auth service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MfaEnrollment {
    pub secret_base32: String,
    pub provisioning_uri: String,
    pub qr_code_base64_png: String,
}

/// Console UI's client for Auth Service's TOTP-based MFA endpoints (ADR-0051): three self-service
/// calls made from an already-authenticated session (`enroll`/`verify`/`disable`, same
/// `x-tenant-id`/`x-username` trust boundary as `UsersClient`), and one pre-session call made
/// mid-login (`challenge`), which trusts nothing but the single-use `challenge_token`
/// `local_login` handed back.
#[async_trait]
pub trait MfaClient: Send + Sync {
    async fn status(&self, tenant_id: Uuid, username: &str) -> Result<bool, MfaClientError>;

    async fn enroll(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<MfaEnrollment, MfaClientError>;

    async fn verify(
        &self,
        tenant_id: Uuid,
        username: &str,
        code: &str,
    ) -> Result<(), MfaClientError>;

    async fn disable(
        &self,
        tenant_id: Uuid,
        username: &str,
        password: &str,
    ) -> Result<(), MfaClientError>;

    async fn challenge(
        &self,
        challenge_token: &str,
        code: &str,
    ) -> Result<(String, Uuid, Role), MfaClientError>;
}

pub struct HttpMfaClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpMfaClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

async fn expect_ok(response: reqwest::Response) -> Result<reqwest::Response, MfaClientError> {
    if !response.status().is_success() {
        return Err(MfaClientError::Rejected(response.status().as_u16()));
    }
    Ok(response)
}

#[async_trait]
impl MfaClient for HttpMfaClient {
    async fn status(&self, tenant_id: Uuid, username: &str) -> Result<bool, MfaClientError> {
        let response = self
            .client
            .get(format!("{}/v1/auth/local/mfa/status", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-username", username)
            .send()
            .await
            .map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        let response = expect_ok(response).await?;

        #[derive(serde::Deserialize)]
        struct Body {
            enabled: bool,
        }
        let body: Body =
            response.json().await.map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        Ok(body.enabled)
    }

    async fn enroll(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<MfaEnrollment, MfaClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/mfa/enroll", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-username", username)
            .send()
            .await
            .map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        let response = expect_ok(response).await?;

        #[derive(serde::Deserialize)]
        struct Body {
            secret_base32: String,
            provisioning_uri: String,
            qr_code_base64_png: String,
        }
        let body: Body =
            response.json().await.map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        Ok(MfaEnrollment {
            secret_base32: body.secret_base32,
            provisioning_uri: body.provisioning_uri,
            qr_code_base64_png: body.qr_code_base64_png,
        })
    }

    async fn verify(
        &self,
        tenant_id: Uuid,
        username: &str,
        code: &str,
    ) -> Result<(), MfaClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/mfa/verify", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-username", username)
            .json(&serde_json::json!({"code": code}))
            .send()
            .await
            .map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        expect_ok(response).await?;
        Ok(())
    }

    async fn disable(
        &self,
        tenant_id: Uuid,
        username: &str,
        password: &str,
    ) -> Result<(), MfaClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/mfa/disable", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-username", username)
            .json(&serde_json::json!({"password": password}))
            .send()
            .await
            .map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        expect_ok(response).await?;
        Ok(())
    }

    async fn challenge(
        &self,
        challenge_token: &str,
        code: &str,
    ) -> Result<(String, Uuid, Role), MfaClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/mfa/challenge", self.auth_service_url))
            .json(&serde_json::json!({"challenge_token": challenge_token, "code": code}))
            .send()
            .await
            .map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        let response = expect_ok(response).await?;

        #[derive(serde::Deserialize)]
        struct Body {
            token: String,
            tenant_id: Uuid,
            role: Role,
        }
        let body: Body =
            response.json().await.map_err(|e| MfaClientError::Unreachable(e.to_string()))?;
        Ok((body.token, body.tenant_id, body.role))
    }
}
