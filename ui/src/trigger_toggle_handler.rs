#[path = "trigger_toggle_handler_test.rs"]
#[cfg(test)]
mod trigger_toggle_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use uuid::Uuid;

/// POST /triggers/:id/toggle — flips a trigger's enabled flag, same shape as
/// `post_toggle_retention_policy`/`AgentsClient`'s toggle convention: fetch the current
/// definition, flip `enabled`, PUT the whole record back (config-admin-service's `update`
/// replaces the row and audit-logs the change). Trigger definitions previously had no
/// operator-facing way to disable one short of a raw API call bypassing the UI's session/RBAC
/// layer and audit-log actor attribution.
pub async fn post_toggle_trigger(
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

    if let Ok(Some(mut trigger)) = state.triggers_client.get_trigger(session.tenant_id, id).await {
        trigger.enabled = !trigger.enabled;
        let _ =
            state.triggers_client.update_trigger(session.role, &session.username, trigger).await;
    }
    Redirect::to("/triggers").into_response()
}
