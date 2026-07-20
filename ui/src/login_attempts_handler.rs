#[path = "login_attempts_handler_test.rs"]
#[cfg(test)]
mod login_attempts_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use common::Role;

struct LoginAttemptRow {
    username: String,
    success: bool,
    reason: String,
    attempted_at: DateTime<Utc>,
}

#[derive(Template)]
#[template(path = "login_attempts.html")]
struct LoginAttemptsTemplate {
    show_nav: bool,
    attempts: Vec<LoginAttemptRow>,
    failed_count: usize,
    error: Option<String>,
}

async fn require_admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::Session, Response> {
    let session = require_session(state.session_store.as_ref(), headers).await?;
    if !session.role.at_least(Role::Admin) {
        return Err(axum::http::StatusCode::FORBIDDEN.into_response());
    }
    Ok(session)
}

/// GET /security/login-attempts — every recent local-login and MFA-challenge attempt for the
/// tenant, successful or not (ADR-0053), the enterprise-compliance "can we see a brute-force
/// pattern or a specific account under attack" question the audit log alone can't answer (the
/// audit log only ever records *changes*, and a failed login is deliberately not one -- there's
/// no entity to attach it to). `Admin`-only, matching Active Sessions' access bar.
pub async fn get_login_attempts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.login_attempts_client.list_recent(session.tenant_id, session.role).await {
        Ok(attempts) => {
            let failed_count = attempts.iter().filter(|a| !a.success).count();
            let rows = attempts
                .into_iter()
                .map(|a| LoginAttemptRow {
                    username: a.username,
                    success: a.success,
                    reason: a.reason,
                    attempted_at: a.attempted_at,
                })
                .collect();
            Html(
                LoginAttemptsTemplate { show_nav: true, attempts: rows, failed_count, error: None }
                    .render()
                    .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            LoginAttemptsTemplate {
                show_nav: true,
                attempts: vec![],
                failed_count: 0,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
