#[path = "service_account_handler_test.rs"]
#[cfg(test)]
mod service_account_handler_test;

use crate::local_login_handler::AuthState;
use crate::local_user_repository::{LocalUserRepositoryError, ServiceAccount};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::Utc;
use common::Role;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct CreateServiceAccountRequest {
    pub label: String,
    pub role: Role,
}

#[derive(serde::Serialize)]
pub struct CreatedServiceAccountResponse {
    pub account: ServiceAccount,
    pub token: String,
}

#[derive(serde::Serialize)]
pub struct ServiceAccountPrincipal {
    pub service_account_id: Uuid,
    pub tenant_id: Uuid,
    pub label: String,
    pub role: Role,
    pub query_token: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({"error": message.into()}))).into_response()
}

#[allow(clippy::result_large_err)]
fn header_uuid(headers: &HeaderMap, name: &str) -> Result<Uuid, Response> {
    let value = headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, format!("missing {name} header")))?;
    Uuid::parse_str(value)
        .map_err(|_| error(StatusCode::BAD_REQUEST, format!("{name} is not a valid UUID")))
}

#[allow(clippy::result_large_err)]
fn actor(headers: &HeaderMap) -> Result<String, Response> {
    headers
        .get("x-username")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "missing X-Username header"))
}

#[allow(clippy::result_large_err)]
fn admin(headers: &HeaderMap) -> Result<(), Response> {
    match headers.get("x-role").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()) {
        Some(Role::Admin) => Ok(()),
        _ => Err(error(
            StatusCode::FORBIDDEN,
            "role does not have permission to manage service accounts",
        )),
    }
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub async fn list_service_accounts(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let tenant_id = match header_uuid(&headers, "x-tenant-id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = admin(&headers) {
        return e;
    }
    match state.local_user_repository.list_service_accounts(tenant_id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => repository_error(e),
    }
}

pub async fn create_service_account(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(req): Json<CreateServiceAccountRequest>,
) -> Response {
    let tenant_id = match header_uuid(&headers, "x-tenant-id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let actor = match actor(&headers) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = admin(&headers) {
        return e;
    }
    let label = req.label.trim();
    if label.is_empty() || label.len() > 120 {
        return error(StatusCode::BAD_REQUEST, "label must be between 1 and 120 characters");
    }
    let token = format!("kzsh_sa_{}_{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let account = ServiceAccount {
        id: Uuid::new_v4(),
        tenant_id,
        label: label.to_string(),
        role: req.role,
        created_at: Utc::now(),
        revoked_at: None,
    };
    match state
        .local_user_repository
        .create_service_account(account.clone(), &hash_token(&token), &actor)
        .await
    {
        Ok(()) => (StatusCode::CREATED, Json(CreatedServiceAccountResponse { account, token }))
            .into_response(),
        Err(e) => repository_error(e),
    }
}

pub async fn revoke_service_account(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match header_uuid(&headers, "x-tenant-id") {
        Ok(v) => v,
        Err(e) => return e,
    };
    let actor = match actor(&headers) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if let Err(e) = admin(&headers) {
        return e;
    }
    match state.local_user_repository.revoke_service_account(tenant_id, id, &actor).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => repository_error(e),
    }
}

pub async fn introspect_service_account(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> Response {
    let Some(value) = headers.get("authorization").and_then(|v| v.to_str().ok()) else {
        return error(StatusCode::UNAUTHORIZED, "missing bearer token");
    };
    let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))
    else {
        return error(StatusCode::UNAUTHORIZED, "expected bearer token");
    };
    if token.is_empty() {
        return error(StatusCode::UNAUTHORIZED, "missing bearer token");
    }
    match state.local_user_repository.find_service_account_by_token_hash(&hash_token(token)).await {
        Ok(Some(account)) if account.revoked_at.is_none() => {
            let query_token = match state
                .session_client
                .mint_session(account.tenant_id, account.role, "service-account")
                .await
            {
                Ok(token) => token,
                Err(exchange_error) => {
                    tracing::error!(error = %exchange_error, "failed to exchange service-account token");
                    return error(StatusCode::BAD_GATEWAY, "failed to establish API session");
                }
            };
            Json(ServiceAccountPrincipal {
                service_account_id: account.id,
                tenant_id: account.tenant_id,
                label: account.label,
                role: account.role,
                query_token,
            })
            .into_response()
        }
        Ok(_) => error(StatusCode::UNAUTHORIZED, "invalid or revoked service-account token"),
        Err(e) => repository_error(e),
    }
}

fn repository_error(repository_error: LocalUserRepositoryError) -> Response {
    match repository_error {
        LocalUserRepositoryError::NotFound(_) => {
            error(StatusCode::NOT_FOUND, "service account not found")
        }
        LocalUserRepositoryError::Backend(message) => {
            tracing::error!(error = %message, "service account repository error");
            error(StatusCode::INTERNAL_SERVER_ERROR, "an internal error occurred")
        }
    }
}
