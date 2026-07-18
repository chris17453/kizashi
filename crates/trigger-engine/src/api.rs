#[path = "api_test.rs"]
#[cfg(test)]
mod api_test;

use crate::trigger_repository::TriggerRepository;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub trigger_repository: Arc<dyn TriggerRepository>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

/// GET /v1/triggers/:id — the API-mediated read path onto TriggerDefinition storage (spec §2
/// principle 1). Action Executor calls this to resolve which actions to run for a firing
/// event, instead of reading Trigger Engine's Postgres schema directly.
async fn get_trigger(State(state): State<ApiState>, Path(id): Path<Uuid>) -> Response {
    match state.trigger_repository.get_by_id(id).await {
        Ok(Some(trigger)) => Json(trigger).into_response(),
        Ok(None) => {
            (StatusCode::NOT_FOUND, Json(ErrorBody { error: format!("no trigger with id {id}") }))
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorBody { error: e.to_string() }))
            .into_response(),
    }
}

pub fn build_router(state: ApiState) -> Router {
    Router::new().route("/v1/triggers/:id", get(get_trigger)).with_state(state)
}
