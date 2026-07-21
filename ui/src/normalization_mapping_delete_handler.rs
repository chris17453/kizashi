#[path = "normalization_mapping_delete_handler_test.rs"]
#[cfg(test)]
mod normalization_mapping_delete_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use uuid::Uuid;

/// POST /normalization-mappings/:id/delete — operator-only (ADR-0110), same shape as
/// `post_delete_trigger`/`post_delete_retention_policy`. config-admin-service's `delete`
/// audit-logs the change and publishes a `MappingChangeEvent::Deleted` so
/// normalization-service's own mirrored copy is removed too.
pub async fn post_delete_normalization_mapping(
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
        .normalization_mappings_client
        .delete_mapping(session.role, &session.username, session.tenant_id, id)
        .await;
    Redirect::to("/normalization-mappings").into_response()
}
