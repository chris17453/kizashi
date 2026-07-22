#[path = "proxy_handler_test.rs"]
#[cfg(test)]
mod proxy_handler_test;

use crate::token_store::TokenStore;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use std::sync::Arc;

#[derive(Clone)]
pub struct GatewayState {
    pub token_store: Arc<dyn TokenStore>,
    pub http_client: reqwest::Client,
    pub dashboard_api_url: String,
    pub ontology_service_url: String,
    pub internal_secret: String,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ")
}

/// Forwards a read request to Dashboard/Query API after resolving the caller's bearer token to
/// a tenant (spec §8: "gateway layer: auth context scopes all downstream queries"). The
/// downstream service trusts `X-Tenant-Id` precisely because only this gateway is allowed to
/// set it — Dashboard API is never reachable directly by a client. `role` is resolved but not
/// enforced here — RBAC v1 (ADR-0016) scopes read-path gating out, since every authenticated
/// user can already read within their own tenant and the gap is smaller than the write-path one.
pub async fn proxy_get(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let token = match bearer_token(&headers) {
        Some(t) if !t.is_empty() => t,
        _ => return error_response(StatusCode::UNAUTHORIZED, "missing Bearer token"),
    };

    let tenant_id = match state.token_store.session_for_token(token).await {
        Ok(Some((tenant_id, _role))) => tenant_id,
        Ok(None) => return error_response(StatusCode::UNAUTHORIZED, "invalid token"),
        Err(e) => {
            tracing::error!(error = %e, "token lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "auth backend error");
        }
    };

    let upstream_base = if uri.path().starts_with("/api/ontology") {
        &state.ontology_service_url
    } else {
        &state.dashboard_api_url
    };

    let upstream_url = format!("{}{}", upstream_base, uri);
    let upstream = state
        .http_client
        .get(&upstream_url)
        .header("x-tenant-id", tenant_id.to_string())
        .send()
        .await;

    match upstream {
        Ok(response) => {
            let status =
                StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let bytes = response.bytes().await.unwrap_or_default();
            (status, bytes).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "upstream unreachable");
            error_response(StatusCode::BAD_GATEWAY, "upstream API unavailable")
        }
    }
}
