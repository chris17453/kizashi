#[path = "analysis_config_handlers_test.rs"]
#[cfg(test)]
mod analysis_config_handlers_test;

use crate::analysis_config_publisher::AnalysisConfigPublisher;
use crate::analysis_config_repository::AnalysisConfigRepository;
use crate::handlers::{require_operator, tenant_id_from_headers, username_from_headers};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use common::{AnalysisConfig, AnalysisProvider};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AnalysisConfigState {
    pub repository: Arc<dyn AnalysisConfigRepository>,
    pub publisher: Arc<dyn AnalysisConfigPublisher>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

/// Read-side shape for GET /v1/analysis-config (RBAC audit fix, see CLAUDE.md §5): a Viewer or
/// any other authenticated caller can read this endpoint, so the stored AI provider `api_key`
/// secret is never included here — only whether one is currently configured. The write path
/// (`PutAnalysisConfigBody`/`put_analysis_config`) is untouched by this struct; it still
/// accepts and stores the real key exactly as before.
#[derive(serde::Serialize)]
struct AnalysisConfigView {
    tenant_id: Uuid,
    prompt: String,
    provider: AnalysisProvider,
    model: Option<String>,
    endpoint: Option<String>,
    /// Always `None` on the read path — never the real secret. Kept as a field (rather than
    /// dropped entirely) so the JSON shape matches the PUT response's field set.
    api_key: Option<String>,
    api_key_configured: bool,
    updated_at: DateTime<Utc>,
}

impl From<AnalysisConfig> for AnalysisConfigView {
    fn from(config: AnalysisConfig) -> Self {
        Self {
            tenant_id: config.tenant_id,
            prompt: config.prompt,
            provider: config.provider,
            model: config.model,
            endpoint: config.endpoint,
            api_key_configured: config.api_key.is_some(),
            api_key: None,
            updated_at: config.updated_at,
        }
    }
}

/// Distinguishes "the client didn't mention `api_key` at all" (keep whatever is already
/// stored) from "the client explicitly sent `api_key: null`" (clear it) from "the client sent
/// a value" (set it). A plain `Option<String>` with `#[serde(default)]` can't tell the first
/// two apart — both deserialize to `None` — which is exactly what would silently wipe a
/// tenant's configured key the first time the redacted GET response (this same fix) is echoed
/// back through a form that leaves the field blank. Doesn't change what a caller who *does*
/// send a value stores; purely additive.
fn deserialize_optional_api_key<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(serde::Deserialize)]
pub struct PutAnalysisConfigBody {
    prompt: String,
    #[serde(default)]
    provider: AnalysisProvider,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_api_key")]
    api_key: Option<Option<String>>,
}

/// GET /v1/analysis-config — the calling tenant's AI analysis prompt (ADR-0019), or `null` if
/// none has been configured yet (today's existing global-analysis behavior). Deliberately no
/// role check, same as the other read endpoints in this service — but as of the RBAC audit fix
/// above, the response never carries the real `api_key` regardless of caller role.
pub async fn get_analysis_config(
    State(state): State<AnalysisConfigState>,
    headers: HeaderMap,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.repository.get(tenant_id).await {
        Ok(config) => Json(config.map(AnalysisConfigView::from)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "analysis config lookup failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}

/// PUT /v1/analysis-config — replaces the calling tenant's AI analysis prompt, operator-only
/// (ADR-0016), and publishes `analysis_config.changed` so Analysis Service picks it up
/// (ADR-0019/ADR-0018 pattern). A publish failure is logged but does not fail the write — the
/// durable config change already happened, matching every other publish-after-write handler
/// in this system.
pub async fn put_analysis_config(
    State(state): State<AnalysisConfigState>,
    headers: HeaderMap,
    Json(body): Json<PutAnalysisConfigBody>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    let actor = match username_from_headers(&headers) {
        Ok(username) => username,
        Err((status, msg)) => return error_response(status, msg),
    };

    // `api_key` omitted entirely from the request body means "leave it as-is" — look up
    // whatever's already stored so a form that only changed the prompt (and, post-redaction,
    // has no way to see or resubmit the real key) doesn't silently clear it.
    let api_key = match body.api_key {
        Some(explicit) => explicit,
        None => match state.repository.get(tenant_id).await {
            Ok(existing) => existing.and_then(|c| c.api_key),
            Err(e) => {
                tracing::error!(error = %e, "analysis config lookup failed");
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "an internal error occurred; check server logs for details",
                );
            }
        },
    };

    let mut config = AnalysisConfig::new(tenant_id, body.prompt);
    config.provider = body.provider;
    config.model = body.model;
    config.endpoint = body.endpoint;
    config.api_key = api_key;
    match state.repository.upsert(config, &actor).await {
        Ok(saved) => {
            if let Err(e) = state.publisher.publish_analysis_config_changed(&saved).await {
                tracing::error!(tenant_id = %saved.tenant_id, error = %e, "failed to publish analysis_config.changed");
            }
            Json(saved).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "analysis config upsert failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "an internal error occurred; check server logs for details",
            )
        }
    }
}
