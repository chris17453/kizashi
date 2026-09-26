use crate::local_login_handler::AuthState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct OidcProviderPolicyResponse {
    pub provider: Option<String>,
    pub available_providers: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateOidcProviderPolicyRequest {
    pub provider: Option<String>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

#[allow(clippy::result_large_err)]
fn tenant_id(headers: &HeaderMap) -> Result<Uuid, Response> {
    let Some(raw) = headers.get("x-tenant-id").and_then(|v| v.to_str().ok()) else {
        return Err(error_response(StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"));
    };
    Uuid::parse_str(raw).map_err(|_| error_response(StatusCode::BAD_REQUEST, "invalid tenant id"))
}

fn require_admin(headers: &HeaderMap) -> Option<Response> {
    let Some(raw) = headers.get("x-role").and_then(|v| v.to_str().ok()) else {
        return Some(error_response(StatusCode::UNAUTHORIZED, "missing X-Role header"));
    };
    match raw.parse::<Role>() {
        Ok(role) if role.at_least(Role::Admin) => None,
        Ok(_) => Some(error_response(StatusCode::FORBIDDEN, "admin role required")),
        Err(_) => Some(error_response(StatusCode::BAD_REQUEST, "invalid role")),
    }
}

#[allow(clippy::result_large_err)]
fn actor(headers: &HeaderMap) -> Result<String, Response> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| error_response(StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

async fn available_providers(state: &AuthState, tenant_id: Uuid) -> Vec<String> {
    let mut providers = state.oidc_clients.keys().cloned().collect::<Vec<_>>();
    if let Ok(configured) = state.tenant_repository.list_tenant_oidc_providers(tenant_id).await {
        providers.extend(configured.into_iter().map(|provider| provider.provider));
    }
    providers.sort();
    providers.dedup();
    providers
}

pub async fn get_oidc_provider_policy(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    let tenant_id = match tenant_id(&headers) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match state.tenant_repository.oidc_provider(tenant_id).await {
        Ok(provider) => Json(OidcProviderPolicyResponse {
            provider,
            available_providers: available_providers(&state, tenant_id).await,
        })
        .into_response(),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

pub async fn put_oidc_provider_policy(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<UpdateOidcProviderPolicyRequest>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }
    let tenant_id = match tenant_id(&headers) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let actor = match actor(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let provider = request.provider.as_deref().map(str::trim).filter(|value| !value.is_empty());
    if let Some(provider) = provider {
        let globally_configured = state.oidc_clients.contains_key(provider);
        let tenant_configured =
            state.tenant_repository.tenant_oidc_config(tenant_id, provider).await.map_err(
                |error| error_response(StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
            );
        let tenant_configured = match tenant_configured {
            Ok(config) => config.is_some(),
            Err(response) => return response,
        };
        if !globally_configured && !tenant_configured {
            return error_response(StatusCode::BAD_REQUEST, "provider is not configured");
        }
    }
    match state.tenant_repository.set_oidc_provider(tenant_id, provider, &actor).await {
        Ok(()) => Json(OidcProviderPolicyResponse {
            provider: provider.map(str::to_string),
            available_providers: available_providers(&state, tenant_id).await,
        })
        .into_response(),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}
