#[path = "auth_client_test.rs"]
#[cfg(test)]
pub(crate) mod auth_client_test;

use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ServiceAccountSummary {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub label: String,
    pub role: Role,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TenantOidcProviderSummary {
    pub provider: String,
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub redirect_url: String,
    pub has_client_secret: bool,
}

#[derive(Debug, Error)]
pub enum AuthClientError {
    #[error("auth service unreachable: {0}")]
    Unreachable(String),
    #[error("invalid credentials")]
    InvalidCredentials,
}

/// A successful `local_login` call either grants a session outright, or (ADR-0051) hands back a
/// short-lived `challenge_token` the caller must complete via `MfaClient::challenge` before a
/// real session exists — the password alone was correct, but that's only the first factor.
#[derive(Debug, Clone, PartialEq)]
pub enum LocalLoginResult {
    LoggedIn { token: String, tenant_id: Uuid, role: Role },
    MfaRequired { challenge_token: String },
    MfaEnrollmentRequired { token: String, tenant_id: Uuid, role: Role },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceAccountPrincipal {
    pub service_account_id: Uuid,
    pub tenant_id: Uuid,
    pub label: String,
    pub role: Role,
    pub query_token: String,
}

/// Console UI's client for Auth Service's local-login endpoint — the browser never talks to
/// Auth Service directly, since session establishment (the `HttpOnly` cookie) is this
/// process's job (ADR-0014).
#[async_trait]
pub trait AuthClient: Send + Sync {
    async fn local_login(
        &self,
        tenant_name: &str,
        username: &str,
        password: &str,
    ) -> Result<LocalLoginResult, AuthClientError>;

    async fn get_mfa_policy(&self, _tenant_id: Uuid, _role: Role) -> Result<bool, AuthClientError> {
        Err(AuthClientError::Unreachable("MFA policy unavailable".to_string()))
    }

    async fn set_mfa_policy(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _required: bool,
        _actor: &str,
    ) -> Result<bool, AuthClientError> {
        Err(AuthClientError::Unreachable("MFA policy unavailable".to_string()))
    }

    async fn get_oidc_provider_policy(
        &self,
        _tenant_id: Uuid,
        _role: Role,
    ) -> Result<(Option<String>, Vec<String>), AuthClientError> {
        Err(AuthClientError::Unreachable("OIDC provider policy unavailable".to_string()))
    }

    async fn set_oidc_provider_policy(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _provider: Option<&str>,
        _actor: &str,
    ) -> Result<Option<String>, AuthClientError> {
        Err(AuthClientError::Unreachable("OIDC provider policy unavailable".to_string()))
    }

    async fn list_tenant_oidc_providers(
        &self,
        _tenant_id: Uuid,
        _role: Role,
    ) -> Result<Vec<TenantOidcProviderSummary>, AuthClientError> {
        Err(AuthClientError::Unreachable("tenant OIDC configuration unavailable".to_string()))
    }

    async fn set_tenant_oidc_provider(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _provider: &str,
        _client_id: &str,
        _client_secret: &str,
        _auth_url: &str,
        _token_url: &str,
        _userinfo_url: &str,
        _redirect_url: &str,
        _actor: &str,
    ) -> Result<Vec<TenantOidcProviderSummary>, AuthClientError> {
        Err(AuthClientError::Unreachable("tenant OIDC configuration unavailable".to_string()))
    }

    async fn delete_tenant_oidc_provider(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _provider: &str,
        _actor: &str,
    ) -> Result<Vec<TenantOidcProviderSummary>, AuthClientError> {
        Err(AuthClientError::Unreachable("tenant OIDC configuration unavailable".to_string()))
    }

    async fn introspect_service_account(
        &self,
        _token: &str,
    ) -> Result<ServiceAccountPrincipal, AuthClientError> {
        Err(AuthClientError::Unreachable("service-account introspection unavailable".to_string()))
    }

    async fn list_service_accounts(
        &self,
        _tenant_id: Uuid,
        _role: Role,
    ) -> Result<Vec<ServiceAccountSummary>, AuthClientError> {
        Err(AuthClientError::Unreachable("service-account management unavailable".to_string()))
    }

    async fn create_service_account(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _label: &str,
        _account_role: Role,
        _actor: &str,
    ) -> Result<(ServiceAccountSummary, String), AuthClientError> {
        Err(AuthClientError::Unreachable("service-account management unavailable".to_string()))
    }

    async fn revoke_service_account(
        &self,
        _tenant_id: Uuid,
        _role: Role,
        _id: Uuid,
        _actor: &str,
    ) -> Result<(), AuthClientError> {
        Err(AuthClientError::Unreachable("service-account management unavailable".to_string()))
    }
}

pub struct HttpAuthClient {
    client: reqwest::Client,
    auth_service_url: String,
}

impl HttpAuthClient {
    pub fn new(client: reqwest::Client, auth_service_url: String) -> Self {
        Self { client, auth_service_url }
    }
}

#[derive(serde::Deserialize)]
struct LoginResponse {
    token: Option<String>,
    tenant_id: Option<Uuid>,
    role: Option<Role>,
    #[serde(default)]
    mfa_required: bool,
    challenge_token: Option<String>,
    #[serde(default)]
    mfa_enrollment_required: bool,
}

#[async_trait]
impl AuthClient for HttpAuthClient {
    async fn local_login(
        &self,
        tenant_name: &str,
        username: &str,
        password: &str,
    ) -> Result<LocalLoginResult, AuthClientError> {
        let response = self
            .client
            .post(format!("{}/v1/auth/local/login", self.auth_service_url))
            .json(&serde_json::json!({"tenant_name": tenant_name, "username": username, "password": password}))
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AuthClientError::InvalidCredentials);
        }
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }

        let body: LoginResponse =
            response.json().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))?;

        if body.mfa_required {
            let challenge_token = body.challenge_token.ok_or_else(|| {
                AuthClientError::Unreachable("missing challenge_token".to_string())
            })?;
            return Ok(LocalLoginResult::MfaRequired { challenge_token });
        }

        if body.mfa_enrollment_required {
            let (token, tenant_id, role) = match (body.token, body.tenant_id, body.role) {
                (Some(token), Some(tenant_id), Some(role)) => (token, tenant_id, role),
                _ => {
                    return Err(AuthClientError::Unreachable(
                        "incomplete MFA enrollment response".to_string(),
                    ))
                }
            };
            return Ok(LocalLoginResult::MfaEnrollmentRequired { token, tenant_id, role });
        }

        let (token, tenant_id, role) = match (body.token, body.tenant_id, body.role) {
            (Some(token), Some(tenant_id), Some(role)) => (token, tenant_id, role),
            _ => return Err(AuthClientError::Unreachable("incomplete login response".to_string())),
        };
        Ok(LocalLoginResult::LoggedIn { token, tenant_id, role })
    }

    async fn get_mfa_policy(&self, tenant_id: Uuid, role: Role) -> Result<bool, AuthClientError> {
        let response = self
            .client
            .get(format!("{}/v1/auth/local/mfa-policy", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            required: bool,
        }
        Ok(response
            .json::<Body>()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?
            .required)
    }

    async fn set_mfa_policy(
        &self,
        tenant_id: Uuid,
        role: Role,
        required: bool,
        actor: &str,
    ) -> Result<bool, AuthClientError> {
        let response = self
            .client
            .put(format!("{}/v1/auth/local/mfa-policy", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({"required": required}))
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            required: bool,
        }
        Ok(response
            .json::<Body>()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?
            .required)
    }

    async fn get_oidc_provider_policy(
        &self,
        tenant_id: Uuid,
        role: Role,
    ) -> Result<(Option<String>, Vec<String>), AuthClientError> {
        let response = self
            .client
            .get(format!("{}/v1/auth/oidc/tenant-provider", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            provider: Option<String>,
            available_providers: Vec<String>,
        }
        let body: Body =
            response.json().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        Ok((body.provider, body.available_providers))
    }

    async fn set_oidc_provider_policy(
        &self,
        tenant_id: Uuid,
        role: Role,
        provider: Option<&str>,
        actor: &str,
    ) -> Result<Option<String>, AuthClientError> {
        let response = self
            .client
            .put(format!("{}/v1/auth/oidc/tenant-provider", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({ "provider": provider }))
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            provider: Option<String>,
        }
        Ok(response
            .json::<Body>()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?
            .provider)
    }

    async fn list_tenant_oidc_providers(
        &self,
        tenant_id: Uuid,
        role: Role,
    ) -> Result<Vec<TenantOidcProviderSummary>, AuthClientError> {
        let response = self
            .client
            .get(format!("{}/v1/auth/oidc/tenant-config", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            providers: Vec<TenantOidcProviderSummary>,
        }
        Ok(response
            .json::<Body>()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?
            .providers)
    }

    async fn set_tenant_oidc_provider(
        &self,
        tenant_id: Uuid,
        role: Role,
        provider: &str,
        client_id: &str,
        client_secret: &str,
        auth_url: &str,
        token_url: &str,
        userinfo_url: &str,
        redirect_url: &str,
        actor: &str,
    ) -> Result<Vec<TenantOidcProviderSummary>, AuthClientError> {
        let response = self.client
            .put(format!("{}/v1/auth/oidc/tenant-config/{provider}", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string()).header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({"client_id":client_id,"client_secret":client_secret,"auth_url":auth_url,"token_url":token_url,"userinfo_url":userinfo_url,"redirect_url":redirect_url}))
            .send().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            providers: Vec<TenantOidcProviderSummary>,
        }
        Ok(response
            .json::<Body>()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?
            .providers)
    }

    async fn delete_tenant_oidc_provider(
        &self,
        tenant_id: Uuid,
        role: Role,
        provider: &str,
        actor: &str,
    ) -> Result<Vec<TenantOidcProviderSummary>, AuthClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/auth/oidc/tenant-config/{provider}", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            providers: Vec<TenantOidcProviderSummary>,
        }
        Ok(response
            .json::<Body>()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?
            .providers)
    }

    async fn introspect_service_account(
        &self,
        token: &str,
    ) -> Result<ServiceAccountPrincipal, AuthClientError> {
        let response = self
            .client
            .get(format!("{}/v1/service-accounts/introspect", self.auth_service_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AuthClientError::InvalidCredentials);
        }
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct ResponseBody {
            service_account_id: Uuid,
            tenant_id: Uuid,
            label: String,
            role: Role,
            query_token: String,
        }
        let body: ResponseBody =
            response.json().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        Ok(ServiceAccountPrincipal {
            service_account_id: body.service_account_id,
            tenant_id: body.tenant_id,
            label: body.label,
            role: body.role,
            query_token: body.query_token,
        })
    }

    async fn list_service_accounts(
        &self,
        tenant_id: Uuid,
        role: Role,
    ) -> Result<Vec<ServiceAccountSummary>, AuthClientError> {
        let response = self
            .client
            .get(format!("{}/v1/service-accounts", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        response.json().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))
    }

    async fn create_service_account(
        &self,
        tenant_id: Uuid,
        role: Role,
        label: &str,
        account_role: Role,
        actor: &str,
    ) -> Result<(ServiceAccountSummary, String), AuthClientError> {
        let response = self
            .client
            .post(format!("{}/v1/service-accounts", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .json(&serde_json::json!({"label": label, "role": account_role}))
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        #[derive(serde::Deserialize)]
        struct Body {
            account: ServiceAccountSummary,
            token: String,
        }
        let body: Body =
            response.json().await.map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        Ok((body.account, body.token))
    }

    async fn revoke_service_account(
        &self,
        tenant_id: Uuid,
        role: Role,
        id: Uuid,
        actor: &str,
    ) -> Result<(), AuthClientError> {
        let response = self
            .client
            .delete(format!("{}/v1/service-accounts/{id}", self.auth_service_url))
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role.to_string())
            .header("x-username", actor)
            .send()
            .await
            .map_err(|e| AuthClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(AuthClientError::Unreachable(format!(
                "unexpected status {}",
                response.status()
            )));
        }
        Ok(())
    }
}
