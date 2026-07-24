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

fn parse_bulk_trigger_ids(raw_body: &[u8]) -> Vec<Uuid> {
    let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body) else {
        return Vec::new();
    };
    pairs
        .into_iter()
        .filter(|(key, _)| key == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect()
}

/// POST /triggers/bulk-toggle — applies one explicit enabled state to selected trigger
/// definitions. Each update deliberately goes through the existing full-definition update
/// path so Config/Admin Service records the real actor and an audit entry per trigger.
pub async fn post_bulk_toggle_triggers(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body).unwrap_or_default();
    let target_enabled =
        pairs.iter().find(|(key, _)| key == "target_enabled").map(|(_, value)| value == "enabled");
    let Some(target_enabled) = target_enabled else {
        return Redirect::to("/triggers?notice=bulk_invalid").into_response();
    };

    let mut updated = 0usize;
    for id in parse_bulk_trigger_ids(&body) {
        if let Ok(Some(mut trigger)) =
            state.triggers_client.get_trigger(session.tenant_id, id).await
        {
            if trigger.enabled != target_enabled {
                trigger.enabled = target_enabled;
                if state
                    .triggers_client
                    .update_trigger(session.role, &session.username, trigger)
                    .await
                    .is_ok()
                {
                    updated += 1;
                }
            }
        }
    }
    let notice = if updated == 0 { "bulk_empty" } else { "bulk_updated" };
    Redirect::to(&format!("/triggers?notice={notice}")).into_response()
}
