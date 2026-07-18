#[path = "oidc_handler_test.rs"]
#[cfg(test)]
mod oidc_handler_test;

use crate::local_login_handler::{AuthState, LoginResponse};
use crate::oidc_client::OidcClient;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;
use uuid::Uuid;

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

/// GET /v1/auth/oidc/:provider/authorize — returns the URL to redirect a browser to, plus the
/// PKCE verifier the caller must hold and send back to /callback (ADR-0009: no session/cookie
/// layer here yet, so there is nowhere server-side to stash it between the two hops).
pub async fn authorize(State(state): State<AuthState>, Path(provider): Path<String>) -> Response {
    let client = match state.oidc_clients.get(&provider) {
        Some(client) => client,
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                format!("unknown OIDC provider `{provider}`"),
            )
        }
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
    pub tenant_id: Uuid,
}

/// POST /v1/auth/oidc/:provider/callback — completes the code-for-token exchange, fetches the
/// user's identity, and mints a session the same way local login does.
pub async fn callback(
    State(state): State<AuthState>,
    Path(provider): Path<String>,
    Json(req): Json<OidcCallbackRequest>,
) -> Response {
    let client = match state.oidc_clients.get(&provider) {
        Some(client) => client,
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                format!("unknown OIDC provider `{provider}`"),
            )
        }
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

    match state
        .session_client
        .mint_session(req.tenant_id, &format!("oidc:{provider}:{}", userinfo.subject))
        .await
    {
        Ok(token) => Json(LoginResponse { token, tenant_id: req.tenant_id }).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session mint failed");
            error_response(StatusCode::BAD_GATEWAY, "failed to establish session")
        }
    }
}

pub type OidcClients = std::collections::HashMap<String, Arc<dyn OidcClient>>;
