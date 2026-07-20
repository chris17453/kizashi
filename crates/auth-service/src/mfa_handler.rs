#[path = "mfa_handler_test.rs"]
#[cfg(test)]
mod mfa_handler_test;

use crate::local_login_handler::{AuthState, LoginResponse};
use crate::mfa;
use crate::password::verify_password;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<uuid::Uuid, (StatusCode, &'static str)> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    uuid::Uuid::parse_str(raw)
        .map_err(|_| (StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}

fn username_from_headers(headers: &HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

#[derive(serde::Serialize)]
pub struct MfaEnrollResponse {
    secret_base32: String,
    provisioning_uri: String,
    qr_code_base64_png: String,
}

#[derive(serde::Serialize)]
pub struct MfaStatusResponse {
    enabled: bool,
}

/// GET /v1/auth/local/mfa/status — lets the Console UI's settings page show whether the
/// caller's own account currently has MFA enabled, without exposing the secret itself.
pub async fn get_mfa_status(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.local_user_repository.find_by_username(tenant_id, &username).await {
        Ok(Some(user)) => Json(MfaStatusResponse { enabled: user.mfa_enabled }).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "no such user"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// POST /v1/auth/local/mfa/enroll — always self-service (the caller's own identity, from
/// X-Tenant-Id/X-Username, same as every other Console-UI-mediated call) and always generates a
/// brand new secret, overwriting any prior pending-but-unconfirmed one -- `mfa_enabled` stays
/// `false` until `mfa/verify` proves the caller's authenticator app actually has it (ADR-0051).
pub async fn post_mfa_enroll(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    let user = match state.local_user_repository.find_by_username(tenant_id, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "no such user"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let enrollment = match mfa::generate_enrollment(&username) {
        Ok(enrollment) => enrollment,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };

    if let Err(e) =
        state.local_user_repository.set_pending_mfa_secret(user.id, &enrollment.secret_base32).await
    {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    Json(MfaEnrollResponse {
        secret_base32: enrollment.secret_base32,
        provisioning_uri: enrollment.provisioning_uri,
        qr_code_base64_png: enrollment.qr_code_base64_png,
    })
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct MfaCodeRequest {
    code: String,
}

/// POST /v1/auth/local/mfa/verify — confirms enrollment by checking a code against the pending
/// secret `mfa/enroll` just stored, and only then flips `mfa_enabled` (ADR-0051).
pub async fn post_mfa_verify(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<MfaCodeRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    let user = match state.local_user_repository.find_by_username(tenant_id, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "no such user"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let Some(secret) = &user.mfa_secret else {
        return error_response(StatusCode::BAD_REQUEST, "no MFA enrollment in progress");
    };
    if !mfa::verify_code(secret, &username, &req.code) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid code");
    }

    if let Err(e) = state.local_user_repository.confirm_mfa(user.id).await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    StatusCode::OK.into_response()
}

#[derive(serde::Deserialize)]
pub struct MfaDisableRequest {
    password: String,
}

/// POST /v1/auth/local/mfa/disable — requires re-entering the account password (not just an
/// already-established session) so a hijacked but still-logged-in browser tab can't silently
/// strip a second factor off the account.
pub async fn post_mfa_disable(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<MfaDisableRequest>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    let username = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    let user = match state.local_user_repository.find_by_username(tenant_id, &username).await {
        Ok(Some(user)) => user,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "no such user"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    if !verify_password(&req.password, &user.password_hash) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid password");
    }

    if let Err(e) = state.local_user_repository.disable_mfa(user.id).await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    StatusCode::OK.into_response()
}

#[derive(serde::Deserialize)]
pub struct MfaChallengeRequest {
    challenge_token: String,
    code: String,
}

/// POST /v1/auth/local/mfa/challenge — the second step of a login for a user with MFA enabled
/// (ADR-0051). Deliberately takes no X-Tenant-Id/X-Username: at this point in the flow Console
/// UI has no session or verified identity yet, only the opaque `challenge_token` `local_login`
/// handed back after the password check passed. The token is consumed (single-use) regardless
/// of outcome, so a guessed/stolen token can't be brute-forced across multiple code attempts.
pub async fn post_mfa_challenge(
    State(state): State<AuthState>,
    Json(req): Json<MfaChallengeRequest>,
) -> Response {
    let (user_id, tenant_id) =
        match state.mfa_challenge_repository.consume(&req.challenge_token).await {
            Ok(Some(pair)) => pair,
            Ok(None) => {
                return error_response(StatusCode::UNAUTHORIZED, "invalid or expired challenge")
            }
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

    let user = match state.local_user_repository.find_by_id(user_id).await {
        Ok(Some(user)) if user.tenant_id == tenant_id => user,
        Ok(_) => return error_response(StatusCode::UNAUTHORIZED, "invalid or expired challenge"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let Some(secret) = &user.mfa_secret else {
        return error_response(StatusCode::UNAUTHORIZED, "invalid or expired challenge");
    };
    if !user.mfa_enabled || !mfa::verify_code(secret, &user.username, &req.code) {
        crate::local_login_handler::record_attempt(
            &state,
            Some(user.tenant_id),
            &user.username,
            false,
            "mfa_code_invalid",
        )
        .await;
        return error_response(StatusCode::UNAUTHORIZED, "invalid code");
    }

    match state.session_client.mint_session(user.tenant_id, user.role, "local-login").await {
        Ok(token) => {
            crate::local_login_handler::record_attempt(
                &state,
                Some(user.tenant_id),
                &user.username,
                true,
                "mfa_success",
            )
            .await;
            Json(LoginResponse {
                token,
                tenant_id: user.tenant_id,
                role: user.role,
                username: None,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "session mint failed");
            error_response(StatusCode::BAD_GATEWAY, "failed to establish session")
        }
    }
}
