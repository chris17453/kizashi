use crate::local_login_handler::AuthState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct TenantOidcConfigResponse {
    pub providers: Vec<crate::tenant_repository::TenantOidcProviderSummary>,
}

#[derive(Deserialize)]
pub struct TenantOidcConfigRequest {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub redirect_url: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

#[allow(clippy::result_large_err)]
fn admin_context(headers: &HeaderMap) -> Result<(Uuid, String), Response> {
    let tenant_raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    let tenant = Uuid::parse_str(tenant_raw)
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid tenant id"))?;
    let role = headers
        .get("x-role")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "missing X-Role header"))?
        .parse::<Role>()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid role"))?;
    if !role.at_least(Role::Admin) {
        return Err(error_response(StatusCode::FORBIDDEN, "admin role required"));
    }
    let actor = headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "missing X-Username header"))?
        .to_string();
    Ok((tenant, actor))
}

#[allow(clippy::result_large_err)]
fn validate_provider(provider: &str, request: &TenantOidcConfigRequest) -> Result<(), Response> {
    if provider.is_empty()
        || provider.len() > 64
        || !provider
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "provider must use 1-64 lowercase letters, digits, '-' or '_'",
        ));
    }
    for (label, value) in [
        ("client_id", &request.client_id),
        ("client_secret", &request.client_secret),
        ("auth_url", &request.auth_url),
        ("token_url", &request.token_url),
        ("userinfo_url", &request.userinfo_url),
        ("redirect_url", &request.redirect_url),
    ] {
        if value.trim().is_empty() {
            return Err(error_response(StatusCode::BAD_REQUEST, format!("{label} is required")));
        }
    }
    let config = crate::OidcProviderConfig {
        client_id: request.client_id.clone(),
        client_secret: request.client_secret.clone(),
        auth_url: request.auth_url.clone(),
        token_url: request.token_url.clone(),
        userinfo_url: request.userinfo_url.clone(),
        redirect_url: request.redirect_url.clone(),
    };
    crate::StandardOidcClient::new(config)
        .map(|_| ())
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, e.to_string()))
}

pub async fn get_tenant_oidc_configs(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> Response {
    let (tenant, _) = match admin_context(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match state.tenant_repository.list_tenant_oidc_providers(tenant).await {
        Ok(providers) => Json(TenantOidcConfigResponse { providers }).into_response(),
        Err(error) => error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
    }
}

pub async fn put_tenant_oidc_config(
    State(state): State<AuthState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Json(request): Json<TenantOidcConfigRequest>,
) -> Response {
    let (tenant, actor) = match admin_context(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if let Err(response) = validate_provider(&provider, &request) {
        return response;
    }
    let config = crate::OidcProviderConfig {
        client_id: request.client_id,
        client_secret: request.client_secret,
        auth_url: request.auth_url,
        token_url: request.token_url,
        userinfo_url: request.userinfo_url,
        redirect_url: request.redirect_url,
    };
    match state
        .tenant_repository
        .upsert_tenant_oidc_provider(tenant, &provider, &config, &actor)
        .await
    {
        Ok(()) => get_tenant_oidc_configs(State(state), headers).await,
        Err(error) => error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
    }
}

pub async fn delete_tenant_oidc_config(
    State(state): State<AuthState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (tenant, actor) = match admin_context(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match state.tenant_repository.delete_tenant_oidc_provider(tenant, &provider, &actor).await {
        Ok(()) => get_tenant_oidc_configs(State(state), headers).await,
        Err(error) => error_response(StatusCode::NOT_FOUND, error.to_string()),
    }
}
