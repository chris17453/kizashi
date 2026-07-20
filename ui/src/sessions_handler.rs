#[path = "sessions_handler_test.rs"]
#[cfg(test)]
mod sessions_handler_test;

use crate::session_guard::{require_session, session_cookie_value};
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use common::Role;

struct SessionRow {
    id: String,
    username: String,
    role_str: String,
    created_at: DateTime<Utc>,
    is_current: bool,
}

#[derive(Template)]
#[template(path = "sessions.html")]
struct SessionsTemplate {
    show_nav: bool,
    sessions: Vec<SessionRow>,
}

async fn require_admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::Session, Response> {
    let session = require_session(state.session_store.as_ref(), headers).await?;
    if !session.role.at_least(Role::Admin) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(session)
}

/// GET /security/sessions — every active session for the caller's tenant (ADR-0046), the
/// enterprise-security expectation of being able to see, and force-terminate, who currently
/// holds a live login -- e.g. after an employee leaves, or to investigate a suspected
/// compromised account. `Admin`-only, matching `/users`' access bar (ADR-0016 follow-up):
/// seeing every session in the tenant (not just your own) is a step above ordinary write access.
pub async fn get_sessions(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let current_id = session_cookie_value(&headers);

    let mut sessions: Vec<SessionRow> = state
        .session_store
        .list_for_tenant(session.tenant_id)
        .await
        .into_iter()
        .map(|(id, s)| SessionRow {
            is_current: current_id.as_deref() == Some(id.as_str()),
            id,
            username: s.username,
            role_str: s.role.to_string(),
            created_at: s.created_at,
        })
        .collect();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.created_at));

    Html(SessionsTemplate { show_nav: true, sessions }.render().unwrap()).into_response()
}

/// POST /security/sessions/:id/revoke — force-terminates one session. Only a session already
/// confirmed to belong to the caller's own tenant (via `list_for_tenant`) is ever deleted, so an
/// admin can't blind-guess another tenant's session id to log someone else's user out.
pub async fn post_revoke_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let belongs_to_tenant = state
        .session_store
        .list_for_tenant(session.tenant_id)
        .await
        .into_iter()
        .any(|(session_id, _)| session_id == id);
    if belongs_to_tenant {
        state.session_store.delete(&id).await;
    }

    Redirect::to("/security/sessions").into_response()
}
