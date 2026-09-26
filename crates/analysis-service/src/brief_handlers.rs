#[path = "brief_handlers_test.rs"]
#[cfg(test)]
mod brief_handlers_test;

use async_trait::async_trait;
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{routing::post, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait IncidentBriefGenerator: Send + Sync {
    async fn generate(
        &self,
        tenant_id: Uuid,
        evidence: serde_json::Value,
    ) -> Result<String, String>;
}

#[derive(Clone)]
pub struct BriefState {
    pub generator: Arc<dyn IncidentBriefGenerator>,
    pub internal_secret: String,
}

pub struct AnalysisIncidentBriefGenerator {
    pub deps: crate::AnalysisDeps,
}

#[async_trait]
impl IncidentBriefGenerator for AnalysisIncidentBriefGenerator {
    async fn generate(
        &self,
        tenant_id: Uuid,
        evidence: serde_json::Value,
    ) -> Result<String, String> {
        crate::generate_incident_brief(&self.deps, tenant_id, evidence)
            .await
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct BriefRequest {
    pub evidence: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct BriefResponse {
    pub summary: String,
}

pub fn build_router(state: BriefState) -> Router {
    Router::new().route("/v1/incident-brief", post(generate_brief)).with_state(state)
}

fn authorized(state: &BriefState, headers: &HeaderMap) -> bool {
    headers
        .get("x-internal-secret")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.internal_secret)
}

pub async fn generate_brief(
    State(state): State<BriefState>,
    headers: HeaderMap,
    Json(request): Json<BriefRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let tenant_id = match headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
    {
        Some(tenant_id) => tenant_id,
        None => return (StatusCode::BAD_REQUEST, "invalid tenant").into_response(),
    };
    if !request.evidence.is_object() {
        return (StatusCode::BAD_REQUEST, "evidence must be an object").into_response();
    }
    match state.generator.generate(tenant_id, request.evidence).await {
        Ok(summary) => Json(BriefResponse { summary }).into_response(),
        Err(error) => {
            tracing::warn!(%tenant_id, error = %error, "incident brief generation failed");
            (StatusCode::SERVICE_UNAVAILABLE, "incident brief unavailable").into_response()
        }
    }
}
