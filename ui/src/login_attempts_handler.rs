#[path = "login_attempts_handler_test.rs"]
#[cfg(test)]
mod login_attempts_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use common::Role;

/// Matches `PostgresLoginAttemptRepository::list_recent`'s `DEFAULT_LIMIT` (auth-service,
/// ADR-0053) -- a full page from the backend is exactly this many rows, so this is also the
/// signal `get_login_attempts` uses to decide whether a "Load older" link makes sense.
const DEFAULT_LIMIT: usize = 50;

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
    next_before: Option<DateTime<Utc>>,
    q: String,
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

#[derive(serde::Deserialize, Default)]
pub struct LoginAttemptsQuery {
    #[serde(default)]
    q: String,
    before: Option<DateTime<Utc>>,
}

/// Case-insensitive substring match on username -- same in-handler-filter shape as the other
/// list-page searches (ADR-0062). Applied to one already-fetched page at a time (see
/// `get_login_attempts`'s doc comment for the resulting caveat on a naturally paginated feed).
fn matches_query(row: &LoginAttemptRow, q: &str) -> bool {
    q.is_empty() || row.username.to_lowercase().contains(&q.to_lowercase())
}

/// GET /security/login-attempts — every recent local-login and MFA-challenge attempt for the
/// tenant, successful or not (ADR-0053), the enterprise-compliance "can we see a brute-force
/// pattern or a specific account under attack" question the audit log alone can't answer (the
/// audit log only ever records *changes*, and a failed login is deliberately not one -- there's
/// no entity to attach it to). `Admin`-only, matching Active Sessions' access bar.
///
/// Accepts `?before=` (ADR-0063), the same exclusive keyset cursor `/audit-log`'s "Load older"
/// link already uses, since this feed is naturally high-volume for an actively-attacked tenant
/// and was previously capped at one fixed page with no way to see further back. `?q=` filters
/// within whichever page was fetched -- a search that doesn't also page won't find a match
/// sitting on an older page; that's an accepted limitation of filtering client-side of a
/// server-paginated feed, not a bug.
pub async fn get_login_attempts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LoginAttemptsQuery>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state
        .login_attempts_client
        .list_recent(session.tenant_id, session.role, query.before)
        .await
    {
        Ok(attempts) => {
            let next_before = if attempts.len() >= DEFAULT_LIMIT {
                attempts.last().map(|a| a.attempted_at)
            } else {
                None
            };
            let failed_count = attempts.iter().filter(|a| !a.success).count();
            let rows: Vec<LoginAttemptRow> = attempts
                .into_iter()
                .map(|a| LoginAttemptRow {
                    username: a.username,
                    success: a.success,
                    reason: a.reason,
                    attempted_at: a.attempted_at,
                })
                .filter(|row| matches_query(row, &query.q))
                .collect();
            Html(
                LoginAttemptsTemplate {
                    show_nav: true,
                    attempts: rows,
                    failed_count,
                    next_before,
                    q: query.q,
                    error: None,
                }
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
                next_before: None,
                q: query.q,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
