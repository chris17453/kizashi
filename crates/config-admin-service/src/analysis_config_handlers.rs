#[path = "analysis_config_handlers_test.rs"]
#[cfg(test)]
mod analysis_config_handlers_test;

use crate::analysis_config_publisher::AnalysisConfigPublisher;
use crate::analysis_config_repository::{AnalysisConfigRepository, AnalysisConfigRepositoryError};
use crate::handlers::{require_operator, tenant_id_from_headers};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::{AnalysisConfig, AnalysisProvider};
use std::sync::Arc;

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

#[derive(serde::Deserialize)]
pub struct PutAnalysisConfigBody {
    prompt: String,
    #[serde(default)]
    provider: AnalysisProvider,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
}

/// GET /v1/analysis-config — the calling tenant's AI analysis prompt (ADR-0019), or `null` if
/// none has been configured yet (today's existing global-analysis behavior).
pub async fn get_analysis_config(
    State(state): State<AnalysisConfigState>,
    headers: HeaderMap,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };

    match state.repository.get(tenant_id).await {
        Ok(config) => Json(config).into_response(),
        Err(AnalysisConfigRepositoryError::Backend(e)) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, e)
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

    let mut config = AnalysisConfig::new(tenant_id, body.prompt);
    config.provider = body.provider;
    config.model = body.model;
    config.endpoint = body.endpoint;
    config.api_key = body.api_key;
    match state.repository.upsert(config).await {
        Ok(saved) => {
            if let Err(e) = state.publisher.publish_analysis_config_changed(&saved).await {
                tracing::error!(tenant_id = %saved.tenant_id, error = %e, "failed to publish analysis_config.changed");
            }
            Json(saved).into_response()
        }
        Err(AnalysisConfigRepositoryError::Backend(e)) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    }
}
