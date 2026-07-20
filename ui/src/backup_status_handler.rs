#[path = "backup_status_handler_test.rs"]
#[cfg(test)]
mod backup_status_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use common::Role;

/// Matches `backup-service`'s `ops_handlers::DEFAULT_STATUS_LIMIT` -- a page shorter than this
/// means the backend has no more history, so "Load older" shouldn't render (same pattern as
/// `login_attempts_handler`'s `DEFAULT_LIMIT`).
const DEFAULT_STATUS_LIMIT: usize = 20;

struct BackupRunRow {
    started_at: String,
    completed_at: Option<String>,
    status: String,
    target: String,
    size_bytes: Option<i64>,
    error: Option<String>,
}

#[derive(serde::Deserialize, Default)]
pub struct BackupsQuery {
    before: Option<DateTime<Utc>>,
}

#[derive(Template)]
#[template(path = "backups.html")]
struct BackupsTemplate {
    show_nav: bool,
    runs: Vec<BackupRunRow>,
    next_before: Option<DateTime<Utc>>,
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

/// GET /security/backups — the last N backup runs (ADR-0055), so an admin (or an auditor) can
/// answer "did the last backup actually succeed" without SSHing into anything. `Admin`-only,
/// matching Active Sessions/Login Attempts' access bar; platform-wide, not tenant-scoped, since
/// a backup is of the whole database, not one tenant's slice of it.
pub async fn get_backups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<BackupsQuery>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.backup_status_client.list_recent(session.role, query.before).await {
        Ok(runs) => {
            let next_before = if runs.len() >= DEFAULT_STATUS_LIMIT {
                runs.last().map(|r| r.started_at)
            } else {
                None
            };
            Html(
                BackupsTemplate {
                    show_nav: true,
                    runs: runs
                        .into_iter()
                        .map(|r| BackupRunRow {
                            started_at: r.started_at.to_rfc3339(),
                            completed_at: r.completed_at.map(|c| c.to_rfc3339()),
                            status: r.status,
                            target: r.target,
                            size_bytes: r.size_bytes,
                            error: r.error,
                        })
                        .collect(),
                    next_before,
                    error: None,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            BackupsTemplate {
                show_nav: true,
                runs: vec![],
                next_before: None,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
