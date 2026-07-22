use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use common::ontology::ActionInvocation;
use crate::api::ApiState;

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/internal/action-invocations", post(log_invocation))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn log_invocation(
    State(state): State<ApiState>,
    Json(payload): Json<ActionInvocation>,
) -> Result<Json<()>, axum::http::StatusCode> {
    state.repository.insert_action_invocation(payload).await.map_err(|e| {
        tracing::error!("failed to insert action_invocation: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(()))
}
