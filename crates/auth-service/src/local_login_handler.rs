#[path = "local_login_handler_test.rs"]
#[cfg(test)]
mod local_login_handler_test;

use crate::local_user_repository::LocalUserRepository;
use crate::oidc_handler::OidcClients;
use crate::password::verify_password;
use crate::session_client::SessionClient;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthState {
    pub local_user_repository: Arc<dyn LocalUserRepository>,
    pub session_client: Arc<dyn SessionClient>,
    pub oidc_clients: OidcClients,
}

#[derive(serde::Deserialize)]
pub struct LocalLoginRequest {
    pub tenant_id: Uuid,
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// POST /v1/auth/local/login — verifies a username/password against `local_users` (spec §8),
/// then mints a session via Query Gateway's internal API. Deliberately returns the same 401
/// for "unknown username" and "wrong password" — distinguishing them lets an attacker enumerate
/// valid usernames.
pub async fn local_login(
    State(state): State<AuthState>,
    Json(req): Json<LocalLoginRequest>,
) -> Response {
    let user =
        match state.local_user_repository.find_by_username(req.tenant_id, &req.username).await {
            Ok(user) => user,
            Err(e) => {
                tracing::error!(error = %e, "local user lookup failed");
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, "auth backend error");
            }
        };

    let authenticated = match &user {
        Some(user) => verify_password(&req.password, &user.password_hash),
        None => {
            // Still run a verify against a dummy hash so the response-time profile doesn't
            // reveal whether the username exists.
            let _ =
                verify_password(&req.password, "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aGFzaA");
            false
        }
    };

    if !authenticated {
        return error_response(StatusCode::UNAUTHORIZED, "invalid username or password");
    }
    let user = user.expect("authenticated implies user is Some");

    match state.session_client.mint_session(user.tenant_id, "local-login").await {
        Ok(token) => Json(LoginResponse { token }).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session mint failed");
            error_response(StatusCode::BAD_GATEWAY, "failed to establish session")
        }
    }
}
