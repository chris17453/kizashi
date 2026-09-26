#[cfg(test)]
#[path = "handlers_test.rs"]
mod handlers_test;

use crate::{
    parse_projection, ConfirmationResult, ExecutionRepository, ExecutionRepositoryError,
    ExecutionRequestResult, ProjectionClient, WorkflowRepository, WorkflowRepositoryError,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
use common::{PipelineExecution, WorkflowCaseStatus};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct RuntimeState {
    pub repository: Arc<dyn ExecutionRepository>,
    pub projection_client: Option<Arc<dyn ProjectionClient>>,
    pub workflow_repository: Arc<dyn WorkflowRepository>,
    pub internal_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmationRequest {
    pub source_event_id: String,
    pub payload: serde_json::Value,
}

pub fn build_router(state: RuntimeState) -> Router {
    let protected = Router::new()
        .route("/v1/pipeline-executions", post(create_execution))
        .route("/v1/pipeline-executions/:id/confirm", post(confirm_execution))
        .route("/v1/pipeline-executions/:id/review-requirements", post(require_execution_review))
        .route("/v1/workflow-cases", axum::routing::get(list_workflow_cases))
        .route("/v1/workflow-cases/:id/decide", post(decide_workflow_case))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.internal_secret,
            crate::internal_secret::require_internal_secret,
        ));
    protected.route("/healthz", axum::routing::get(crate::healthz))
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({"error": message.into()}))).into_response()
}
#[allow(clippy::result_large_err)]
fn tenant_id(headers: &HeaderMap) -> Result<Uuid, Response> {
    let raw = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "missing X-Tenant-Id header"))?;
    Uuid::parse_str(raw)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "X-Tenant-Id is not a valid UUID"))
}
fn repository_error(error_value: ExecutionRepositoryError) -> Response {
    match error_value {
        ExecutionRepositoryError::NotFound(id) => {
            error(StatusCode::NOT_FOUND, format!("pipeline execution {id} was not found"))
        }
        ExecutionRepositoryError::NotAwaitingConfirmation(id) => error(
            StatusCode::CONFLICT,
            format!("pipeline execution {id} is not awaiting confirmation"),
        ),
        ExecutionRepositoryError::MissingPipelineVersion => {
            error(StatusCode::BAD_REQUEST, "pipeline_version must be positive")
        }
        ExecutionRepositoryError::Backend(_) => {
            error(StatusCode::INTERNAL_SERVER_ERROR, "pipeline runtime storage failure")
        }
    }
}
fn workflow_error(error_value: WorkflowRepositoryError) -> Response {
    match error_value {
        WorkflowRepositoryError::NotFound(id) => {
            error(StatusCode::NOT_FOUND, format!("workflow case {id} was not found"))
        }
        WorkflowRepositoryError::InvalidTransition => error(
            StatusCode::BAD_REQUEST,
            "workflow decision must be approved, rejected, or resolved",
        ),
        WorkflowRepositoryError::Backend(_) => {
            error(StatusCode::INTERNAL_SERVER_ERROR, "workflow storage failure")
        }
    }
}
fn operator(headers: &HeaderMap) -> bool {
    headers
        .get("x-role")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<common::Role>().ok())
        .is_some_and(|role| role.at_least(common::Role::Operator))
}

pub async fn create_execution(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
    Json(mut execution): Json<PipelineExecution>,
) -> Response {
    let tenant_id = match tenant_id(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if execution.tenant_id != tenant_id {
        return error(StatusCode::FORBIDDEN, "tenant_id does not match X-Tenant-Id");
    }
    execution.id = Uuid::new_v4();
    match state.repository.create(execution).await {
        Ok(ExecutionRequestResult::Created(execution)) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"duplicate": false, "execution": execution})),
        )
            .into_response(),
        Ok(ExecutionRequestResult::Duplicate(execution)) => {
            (StatusCode::OK, Json(serde_json::json!({"duplicate": true, "execution": execution})))
                .into_response()
        }
        Err(error_value) => repository_error(error_value),
    }
}

pub async fn confirm_execution(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<ConfirmationRequest>,
) -> Response {
    let tenant_id = match tenant_id(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if request.source_event_id.trim().is_empty() || request.source_event_id.len() > 256 {
        return error(
            StatusCode::BAD_REQUEST,
            "source_event_id must be between 1 and 256 characters",
        );
    }
    let payload = request.payload.clone();
    match state
        .repository
        .record_confirmation(tenant_id, id, &request.source_event_id, payload.clone())
        .await
    {
        Ok(ConfirmationResult::Recorded) => {
            if let Some(client) = &state.projection_client {
                let outcome = match parse_projection(&payload) {
                    Ok(projection) => client
                        .reconcile(tenant_id, &projection)
                        .await
                        .map_err(|_| "projection failed"),
                    Err(message) => Err(message),
                };
                if let Err(message) = outcome {
                    if state
                        .repository
                        .record_reconciliation_failure(
                            tenant_id,
                            id,
                            &request.source_event_id,
                            payload,
                            message,
                        )
                        .await
                        .is_err()
                    {
                        return error(StatusCode::INTERNAL_SERVER_ERROR, "confirmation was recorded but reconciliation failure could not be persisted");
                    }
                    if state
                        .workflow_repository
                        .open_reconciliation_failure(
                            tenant_id,
                            id,
                            &request.source_event_id,
                            message,
                        )
                        .await
                        .is_err()
                    {
                        return error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "confirmation was recorded but workflow review could not be opened",
                        );
                    }
                    return StatusCode::ACCEPTED.into_response();
                }
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(ConfirmationResult::Duplicate) => StatusCode::OK.into_response(),
        Err(error_value) => repository_error(error_value),
    }
}

#[derive(Debug, Deserialize)]
pub struct WorkflowDecisionRequest {
    pub status: WorkflowCaseStatus,
    pub actor: String,
    pub note: Option<String>,
}
pub async fn list_workflow_cases(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
) -> Response {
    let tenant_id = match tenant_id(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match state.workflow_repository.list(tenant_id).await {
        Ok(cases) => Json(cases).into_response(),
        Err(error_value) => workflow_error(error_value),
    }
}
pub async fn decide_workflow_case(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<WorkflowDecisionRequest>,
) -> Response {
    let tenant_id = match tenant_id(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !operator(&headers) {
        return error(StatusCode::FORBIDDEN, "operator access is required for workflow decisions");
    }
    if request.actor.trim().is_empty() || request.actor.len() > 256 {
        return error(StatusCode::BAD_REQUEST, "workflow decision actor is required");
    }
    match state
        .workflow_repository
        .decide(tenant_id, id, request.status, &request.actor, request.note.as_deref())
        .await
    {
        Ok(case) => Json(case).into_response(),
        Err(error_value) => workflow_error(error_value),
    }
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequirementRequest {
    pub summary: String,
    pub sla_hours: i64,
}
pub async fn require_execution_review(
    State(state): State<RuntimeState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<ReviewRequirementRequest>,
) -> Response {
    let tenant_id = match tenant_id(&headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !operator(&headers) {
        return error(StatusCode::FORBIDDEN, "operator access is required for workflow review");
    }
    if request.summary.trim().is_empty() || request.summary.len() > 2_000 {
        return error(
            StatusCode::BAD_REQUEST,
            "workflow review summary must be between 1 and 2000 characters",
        );
    }
    if !(1..=720).contains(&request.sla_hours) {
        return error(
            StatusCode::BAD_REQUEST,
            "workflow review SLA must be between 1 and 720 hours",
        );
    }
    match state
        .workflow_repository
        .open_approval(
            tenant_id,
            id,
            request.summary.trim(),
            chrono::Utc::now() + chrono::Duration::hours(request.sla_hours),
        )
        .await
    {
        Ok(case) => (StatusCode::CREATED, Json(case)).into_response(),
        Err(error_value) => workflow_error(error_value),
    }
}
