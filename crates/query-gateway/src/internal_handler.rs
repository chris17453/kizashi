#[path = "internal_handler_test.rs"]
#[cfg(test)]
mod internal_handler_test;

use crate::proxy_handler::GatewayState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Role;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct MintTokenRequest {
    pub tenant_id: Uuid,
    pub role: Role,
    pub label: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct MintTokenResponse {
    pub token: String,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// POST /internal/tokens — mints a session token for a tenant (ADR-0008/0009). Called by Auth
/// Service after a successful login, never by an end-user client directly. Protected by a
/// shared secret rather than a full service-mesh identity layer, a deliberate v1 stopgap
/// documented in ADR-0009, not a permanent trust model.
pub async fn mint_token(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(req): Json<MintTokenRequest>,
) -> Response {
    let provided = headers.get("x-internal-secret").and_then(|v| v.to_str().ok());
    if provided != Some(state.internal_secret.as_str()) {
        return error_response(StatusCode::UNAUTHORIZED, "invalid internal secret");
    }

    match state.token_store.mint_token(req.tenant_id, req.role, &req.label).await {
        Ok(token) => (StatusCode::CREATED, Json(MintTokenResponse { token })).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
