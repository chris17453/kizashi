#[path = "trigger_delete_handler_test.rs"]
#[cfg(test)]
mod trigger_delete_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use uuid::Uuid;

/// POST /triggers/:id/delete — operator-only (ADR-0109), same shape as
/// `post_delete_retention_policy`. config-admin-service's `delete` audit-logs the change and
/// publishes a `TriggerChangeEvent::Deleted` so trigger-engine's own copy is removed too.
pub async fn post_delete_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let _ = state
        .triggers_client
        .delete_trigger(session.role, &session.username, session.tenant_id, id)
        .await;
    Redirect::to("/triggers").into_response()
}
