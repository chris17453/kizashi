#[path = "oidc_handler_test.rs"]
#[cfg(test)]
mod oidc_handler_test;

use crate::local_login_handler::{AuthState, LoginResponse};
use crate::oidc_client::{OidcClient, StandardOidcClient};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AuthorizeResponse {
    pub authorization_url: String,
    pub csrf_token: String,
    pub code_verifier: String,
}

#[derive(serde::Deserialize, Default)]
pub struct AuthorizeQuery {
    pub tenant_name: Option<String>,
}

/// Checks a tenant's optional provider pin. An unset pin preserves the v1 behavior where any
/// provider configured in the deployment may be selected; a pin prevents a login flow from
/// switching providers between the tenant's login page and callback.
async fn tenant_provider_error(
    state: &AuthState,
    tenant_name: Option<&str>,
    provider: &str,
) -> Option<Response> {
    let tenant_name = tenant_name?;
    let tenant_id = match state.tenant_repository.id_for_name(tenant_name).await {
        Ok(Some(id)) => id,
        Ok(None) => return Some(error_response(StatusCode::BAD_REQUEST, "unknown workspace")),
        Err(e) => {
            tracing::error!(error = %e, "tenant lookup failed");
            return Some(error_response(StatusCode::INTERNAL_SERVER_ERROR, "tenant lookup failed"));
        }
    };
    match state.tenant_repository.oidc_provider(tenant_id).await {
        Ok(Some(pinned)) if pinned != provider => Some(error_response(
            StatusCode::FORBIDDEN,
            format!("workspace requires OIDC provider `{pinned}`"),
        )),
        Ok(_) => None,
        Err(e) => {
            tracing::error!(error = %e, "tenant OIDC policy lookup failed");
            Some(error_response(StatusCode::INTERNAL_SERVER_ERROR, "auth backend error"))
        }
    }
}

#[allow(clippy::result_large_err)]
async fn client_for_tenant(
    state: &AuthState,
    provider: &str,
    tenant_id: Option<uuid::Uuid>,
) -> Result<Option<Arc<dyn OidcClient>>, Response> {
    if let Some(tenant_id) = tenant_id {
        match state.tenant_repository.tenant_oidc_config(tenant_id, provider).await {
            Ok(Some(config)) => {
                let client = StandardOidcClient::new(config).map_err(|e| {
                    error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                })?;
                return Ok(Some(Arc::new(client)));
            }
            Ok(None) => {}
            Err(e) => {
                tracing::error!(error = %e, "tenant OIDC credential lookup failed");
                return Err(error_response(StatusCode::SERVICE_UNAVAILABLE, "auth backend error"));
            }
        }
    }
    Ok(state.oidc_clients.get(provider).cloned())
}

#[allow(clippy::result_large_err)]
async fn tenant_id_for_name(
    state: &AuthState,
    tenant_name: Option<&str>,
) -> Result<Option<uuid::Uuid>, Response> {
    let Some(tenant_name) = tenant_name else { return Ok(None) };
    match state.tenant_repository.id_for_name(tenant_name).await {
        Ok(Some(id)) => Ok(Some(id)),
        Ok(None) => Err(error_response(StatusCode::BAD_REQUEST, "unknown workspace")),
        Err(e) => {
            tracing::error!(error = %e, "tenant lookup failed");
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "tenant lookup failed"))
        }
    }
}

/// GET /v1/auth/oidc/:provider/authorize — returns the URL to redirect a browser to, plus the
/// PKCE verifier the caller must hold and send back to /callback (ADR-0009: no session/cookie
/// layer here yet, so there is nowhere server-side to stash it between the two hops).
pub async fn authorize(
    State(state): State<AuthState>,
    Path(provider): Path<String>,
    Query(query): Query<AuthorizeQuery>,
) -> Response {
    if let Some(response) =
        tenant_provider_error(&state, query.tenant_name.as_deref(), &provider).await
    {
        return response;
    }
    let tenant_id = match tenant_id_for_name(&state, query.tenant_name.as_deref()).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    let client = match client_for_tenant(&state, &provider, tenant_id).await {
        Ok(Some(client)) => client,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                format!("unknown OIDC provider `{provider}`"),
            )
        }
        Err(response) => return response,
    };

    match client.authorization_request() {
        Ok(req) => Json(AuthorizeResponse {
            authorization_url: req.authorization_url,
            csrf_token: req.csrf_token,
            code_verifier: req.code_verifier,
        })
        .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(serde::Deserialize)]
pub struct OidcCallbackRequest {
    pub code: String,
    pub code_verifier: String,
    /// Not `tenant_id` — the browser-facing caller (Console UI) only ever has the workspace
    /// name the user typed on the login page, same as local login's `tenant_name` field.
    /// Resolved to a `tenant_id` server-side here, mirroring `local_login_handler`.
    pub tenant_name: String,
}

/// POST /v1/auth/oidc/:provider/callback — completes the code-for-token exchange, fetches the
/// user's identity, and mints a session the same way local login does.
pub async fn callback(
    State(state): State<AuthState>,
    Path(provider): Path<String>,
    Json(req): Json<OidcCallbackRequest>,
) -> Response {
    if let Some(response) = tenant_provider_error(&state, Some(&req.tenant_name), &provider).await {
        return response;
    }

    let tenant_id = match state.tenant_repository.id_for_name(&req.tenant_name).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return error_response(StatusCode::BAD_REQUEST, "unknown workspace");
        }
        Err(e) => {
            tracing::error!(error = %e, "tenant lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "tenant lookup failed");
        }
    };

    let client = match client_for_tenant(&state, &provider, Some(tenant_id)).await {
        Ok(Some(client)) => client,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                format!("unknown OIDC provider `{provider}`"),
            )
        }
        Err(response) => return response,
    };

    let access_token = match client.exchange_code(&req.code, &req.code_verifier).await {
        Ok(token) => token,
        Err(e) => {
            tracing::error!(error = %e, "oidc code exchange failed");
            return error_response(StatusCode::BAD_GATEWAY, "code exchange failed");
        }
    };

    let userinfo = match client.fetch_userinfo(&access_token).await {
        Ok(info) => info,
        Err(e) => {
            tracing::error!(error = %e, "oidc userinfo fetch failed");
            return error_response(StatusCode::BAD_GATEWAY, "userinfo fetch failed");
        }
    };

    // OIDC has no local role source yet (ADR-0016 scopes RBAC v1 to `local_users`, which OIDC
    // sessions never consult) — default to the least-privileged role rather than leaving OIDC
    // logins unroled or guessing something more permissive.
    let role = common::Role::Viewer;
    match state
        .session_client
        .mint_session(tenant_id, role, &format!("oidc:{provider}:{}", userinfo.subject))
        .await
    {
        Ok(token) => {
            let username = userinfo.email.unwrap_or(userinfo.subject);
            Json(LoginResponse { token, tenant_id, role, username: Some(username) }).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "session mint failed");
            error_response(StatusCode::BAD_GATEWAY, "failed to establish session")
        }
    }
}

pub type OidcClients = std::collections::HashMap<String, Arc<dyn OidcClient>>;
