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

struct BackupTimelinePoint {
    label: String,
    status: String,
    size_label: String,
}

#[derive(serde::Deserialize, Default)]
pub struct BackupsQuery {
    #[serde(default)]
    notice: String,
    before: Option<DateTime<Utc>>,
    #[serde(default)]
    status: String,
}

#[derive(Template)]
#[template(path = "backups.html")]
struct BackupsTemplate {
    show_nav: bool,
    is_admin: bool,
    runs: Vec<BackupRunRow>,
    next_before: Option<DateTime<Utc>>,
    error: Option<String>,
    notice: String,
    successful_count: usize,
    failed_count: usize,
    running_count: usize,
    total_size_bytes: i64,
    timeline: Vec<BackupTimelinePoint>,
    last_success_at: Option<String>,
    freshness_label: String,
    status: String,
}

fn normalize_backup_status(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "success" => "success".to_string(),
        "failed" => "failed".to_string(),
        "running" | "in_progress" => "running".to_string(),
        _ => String::new(),
    }
}

fn recovery_freshness(rows: &[BackupRunRow]) -> (Option<String>, String) {
    let last_success = rows
        .iter()
        .filter(|run| run.status == "success")
        .filter_map(|run| {
            run.completed_at
                .as_deref()
                .or(Some(run.started_at.as_str()))
                .and_then(|value| value.parse::<DateTime<Utc>>().ok())
        })
        .max();
    let Some(last_success) = last_success else {
        return (None, "No successful run".to_string());
    };
    let age = (Utc::now() - last_success).num_hours().max(0);
    let label = if age < 24 {
        "Fresh · under 24h"
    } else if age < 24 * 7 {
        "Aging · under 7d"
    } else {
        "Stale · over 7d"
    };
    (Some(last_success.to_rfc3339()), label.to_string())
}

fn timeline_points(rows: &[BackupRunRow]) -> Vec<BackupTimelinePoint> {
    rows.iter()
        .rev()
        .map(|run| BackupTimelinePoint {
            label: run
                .started_at
                .parse::<DateTime<Utc>>()
                .map(|value| value.format("%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|_| run.started_at.clone()),
            status: run.status.clone(),
            size_label: run
                .size_bytes
                .map(|size| format!("{size} bytes"))
                .unwrap_or_else(|| "no artifact".to_string()),
        })
        .collect()
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
    let is_admin = session.role.at_least(common::Role::Admin);

    match state.backup_status_client.list_recent(session.role, query.before).await {
        Ok(runs) => {
            let next_before = if runs.len() >= DEFAULT_STATUS_LIMIT {
                runs.last().map(|r| r.started_at)
            } else {
                None
            };
            let rows: Vec<BackupRunRow> = runs
                .into_iter()
                .map(|r| BackupRunRow {
                    started_at: r.started_at.to_rfc3339(),
                    completed_at: r.completed_at.map(|c| c.to_rfc3339()),
                    status: r.status,
                    target: r.target,
                    size_bytes: r.size_bytes,
                    error: r.error,
                })
                .filter(|run| {
                    query.status.is_empty() || normalize_backup_status(&run.status) == query.status
                })
                .collect();
            let successful_count = rows.iter().filter(|run| run.status == "success").count();
            let failed_count = rows.iter().filter(|run| run.status == "failed").count();
            let running_count = rows.len().saturating_sub(successful_count + failed_count);
            let total_size_bytes = rows.iter().filter_map(|run| run.size_bytes).sum();
            let timeline = timeline_points(&rows);
            let (last_success_at, freshness_label) = recovery_freshness(&rows);
            Html(
                BackupsTemplate {
                    show_nav: true,
                    is_admin,
                    runs: rows,
                    next_before,
                    error: None,
                    notice: query.notice.clone(),
                    successful_count,
                    failed_count,
                    running_count,
                    total_size_bytes,
                    timeline,
                    last_success_at,
                    freshness_label,
                    status: query.status,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            BackupsTemplate {
                show_nav: true,
                is_admin,
                runs: vec![],
                next_before: None,
                error: Some(e.to_string()),
                notice: query.notice,
                successful_count: 0,
                failed_count: 0,
                running_count: 0,
                total_size_bytes: 0,
                timeline: vec![],
                last_success_at: None,
                freshness_label: "Unavailable".to_string(),
                status: query.status,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

pub async fn post_trigger_backup(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let _session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.backup_status_client.trigger_backup().await {
        Ok(result) if result.status == "success" => {
            axum::response::Redirect::to("/security/backups?notice=triggered").into_response()
        }
        Ok(_) | Err(_) => {
            axum::response::Redirect::to("/security/backups?notice=trigger-failed").into_response()
        }
    }
}
