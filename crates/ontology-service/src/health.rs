use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use common::ontology::ActionInvocation;
use sqlx::PgPool;
use uuid::Uuid;

pub fn build_router(pool: PgPool) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/internal/action-invocations", post(log_invocation))
        .with_state(pool)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn log_invocation(
    State(pool): State<PgPool>,
    Json(payload): Json<ActionInvocation>,
) -> Result<Json<()>, axum::http::StatusCode> {
    sqlx::query(
        r#"
        INSERT INTO action_invocations (id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, executed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(payload.id)
    .bind(payload.tenant_id)
    .bind(payload.action_type_id)
    .bind(payload.target_object_ids)
    .bind(payload.parameters)
    .bind(payload.outcome)
    .bind(payload.triggering_event_ref)
    .bind(payload.executed_at)
    .execute(&pool)
    .await
    .map_err(|e| {
        tracing::error!("failed to insert action_invocation: {}", e);
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(()))
}
