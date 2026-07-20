#[path = "local_login_handler_test.rs"]
#[cfg(test)]
mod local_login_handler_test;

use crate::audit_log::AuditLogReader;
use crate::local_user_repository::LocalUserRepository;
use crate::mfa_repository::MfaChallengeRepository;
use crate::oidc_handler::OidcClients;
use crate::password::verify_password;
use crate::session_client::SessionClient;
use crate::tenant_branding_repository::TenantBrandingRepository;
use crate::tenant_repository::TenantRepository;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthState {
    pub local_user_repository: Arc<dyn LocalUserRepository>,
    pub tenant_repository: Arc<dyn TenantRepository>,
    pub tenant_branding_repository: Arc<dyn TenantBrandingRepository>,
    pub session_client: Arc<dyn SessionClient>,
    pub oidc_clients: OidcClients,
    pub audit_log_reader: Arc<dyn AuditLogReader>,
    pub mfa_challenge_repository: Arc<dyn MfaChallengeRepository>,
}

#[derive(serde::Deserialize)]
pub struct LocalLoginRequest {
    pub tenant_name: String,
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub tenant_id: Uuid,
    pub role: common::Role,
    /// Only populated by the OIDC callback path (the real identity the IdP asserted) — local
    /// login omits it since the caller already knows the username it just typed in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// What `local_login` returns for a user with MFA enabled (ADR-0051) instead of a real session
/// grant -- the password check already passed, but the caller must still complete
/// `POST /v1/auth/local/mfa/challenge` with `challenge_token` and a current code before Console
/// UI can establish a session.
#[derive(serde::Serialize)]
pub struct MfaRequiredResponse {
    pub mfa_required: bool,
    pub challenge_token: String,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aGFzaA";

/// POST /v1/auth/local/login — resolves the caller-typed workspace name to a `tenant_id`
/// (people can't be expected to know or type a UUID for their own workspace), verifies a
/// username/password against `local_users` (spec §8), then mints a session via Query Gateway's
/// internal API. Deliberately returns the same 401 for "unknown workspace", "unknown username",
/// and "wrong password" — distinguishing any of them lets an attacker enumerate valid
/// workspaces/usernames.
pub async fn local_login(
    State(state): State<AuthState>,
    Json(req): Json<LocalLoginRequest>,
) -> Response {
    let tenant_id = match state.tenant_repository.id_for_name(&req.tenant_name).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, "tenant lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "auth backend error");
        }
    };

    let user = match tenant_id {
        Some(tenant_id) => {
            match state.local_user_repository.find_by_username(tenant_id, &req.username).await {
                Ok(user) => user,
                Err(e) => {
                    tracing::error!(error = %e, "local user lookup failed");
                    return error_response(StatusCode::INTERNAL_SERVER_ERROR, "auth backend error");
                }
            }
        }
        None => None,
    };

    let authenticated = match &user {
        Some(user) => verify_password(&req.password, &user.password_hash),
        None => {
            // Still run a verify against a dummy hash so the response-time profile doesn't
            // reveal whether the workspace/username exists.
            let _ = verify_password(&req.password, DUMMY_HASH);
            false
        }
    };

    if !authenticated {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid workspace, username, or password",
        );
    }
    let user = user.expect("authenticated implies user is Some");

    if user.mfa_enabled {
        return match state.mfa_challenge_repository.create(user.id, user.tenant_id).await {
            Ok(challenge_token) => {
                Json(MfaRequiredResponse { mfa_required: true, challenge_token }).into_response()
            }
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
    }

    match state.session_client.mint_session(user.tenant_id, user.role, "local-login").await {
        Ok(token) => Json(LoginResponse {
            token,
            tenant_id: user.tenant_id,
            role: user.role,
            username: None,
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session mint failed");
            error_response(StatusCode::BAD_GATEWAY, "failed to establish session")
        }
    }
}
