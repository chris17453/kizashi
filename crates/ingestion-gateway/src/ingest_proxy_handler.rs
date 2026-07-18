#[path = "ingest_proxy_handler_test.rs"]
#[cfg(test)]
mod ingest_proxy_handler_test;

use crate::api_key_store::ApiKeyStore;
use crate::rate_limiter::RateLimiter;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct GatewayState {
    pub api_key_store: Arc<dyn ApiKeyStore>,
    pub rate_limiter: Arc<RateLimiter>,
    pub http_client: reqwest::Client,
    pub ingestion_service_url: String,
}

#[derive(Debug, Serialize)]
pub struct GatewayErrorBody {
    pub error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(GatewayErrorBody { error: message.into() })).into_response()
}

/// POST /v1/ingest — the single agent-facing entry point (spec §6, service #2). Authenticates
/// the caller's API key to a tenant, applies that tenant's rate-limit budget, then forwards to
/// Ingestion Service with `tenant_id` set from the *authenticated* identity — never from the
/// caller-supplied request body, so a misconfigured or malicious connector cannot write into a
/// tenant it doesn't hold a key for (spec §8 tenant isolation).
pub async fn ingest_proxy(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let api_key = match headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        Some(key) if !key.is_empty() => key.to_string(),
        _ => return error_response(StatusCode::UNAUTHORIZED, "missing X-Api-Key header"),
    };

    let tenant_id = match state.api_key_store.tenant_for_key(&api_key).await {
        Ok(Some(tenant_id)) => tenant_id,
        Ok(None) => return error_response(StatusCode::UNAUTHORIZED, "invalid API key"),
        Err(e) => {
            tracing::error!(error = %e, "api key lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "auth backend error");
        }
    };

    if !state.rate_limiter.check(tenant_id) {
        return error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
    }

    let mut payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "body must be valid JSON"),
    };
    match payload.as_object_mut() {
        Some(obj) => {
            obj.insert("tenant_id".to_string(), serde_json::json!(tenant_id));
        }
        None => return error_response(StatusCode::BAD_REQUEST, "body must be a JSON object"),
    }

    let upstream = state
        .http_client
        .post(format!("{}/v1/records", state.ingestion_service_url))
        .json(&payload)
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
            tracing::error!(error = %e, "ingestion-service unreachable");
            error_response(StatusCode::BAD_GATEWAY, "upstream ingestion service unavailable")
        }
    }
}
