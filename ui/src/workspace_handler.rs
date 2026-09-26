//! Explicit workspace switching. A workspace change must end the current session first;
//! carrying a bearer token across tenant boundaries would make the tenant scope ambiguous.

#[path = "workspace_handler_test.rs"]
#[cfg(test)]
mod workspace_handler_test;

use crate::session_guard::session_cookie_value;
use crate::{AppState, SESSION_COOKIE_NAME, WORKSPACE_COOKIE_NAME};
use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};

#[derive(Debug, Default, serde::Deserialize)]
pub struct SwitchWorkspaceQuery {
    #[serde(default)]
    tenant_name: String,
}

pub(crate) fn workspace_login_target(tenant_name: &str) -> String {
    let tenant_name = tenant_name.trim();
    if tenant_name.is_empty() {
        return "/login".to_string();
    }
    let query = serde_urlencoded::to_string([("tenant_name", tenant_name)]).unwrap_or_default();
    format!("/login?{query}")
}

/// End the current tenant-scoped session and return to the workspace-aware login page.
pub async fn get_switch_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SwitchWorkspaceQuery>,
) -> Response {
    if let Some(session_id) = session_cookie_value(&headers) {
        state.session_store.delete(&session_id).await;
    }

    let secure = crate::cookie_secure_suffix(crate::cookie_secure());
    let expire_session = format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; Max-Age=0{secure}");
    let expire_workspace = format!("{WORKSPACE_COOKIE_NAME}=; Path=/; Max-Age=0{secure}");
    let target = workspace_login_target(&query.tenant_name);
    let mut response = Redirect::to(&target).into_response();
    response.headers_mut().append(SET_COOKIE, expire_session.parse().unwrap());
    response.headers_mut().append(SET_COOKIE, expire_workspace.parse().unwrap());
    response
}
