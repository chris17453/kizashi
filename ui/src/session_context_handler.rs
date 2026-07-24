#[path = "session_context_handler_test.rs"]
#[cfg(test)]
mod session_context_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Small authenticated context endpoint used by the shell to keep the active operator's
/// identity visible while navigating. It deliberately exposes claims already in the UI session
/// only; no tenant or user search is performed here.
#[derive(Debug, Serialize)]
pub struct SessionContext {
    pub username: String,
    pub role: common::Role,
    pub tenant_id: uuid::Uuid,
    pub workspace: String,
}

fn workspace_from_cookie(headers: &HeaderMap, tenant_id: uuid::Uuid) -> String {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == crate::WORKSPACE_COOKIE_NAME).then(|| value.to_string())
            })
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("tenant-{tenant_id}"))
}

pub async fn get_session_context(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let workspace = workspace_from_cookie(&headers, session.tenant_id);
    Json(SessionContext {
        username: session.username,
        role: session.role,
        tenant_id: session.tenant_id,
        workspace,
    })
    .into_response()
}
