#[path = "ops_handlers_test.rs"]
#[cfg(test)]
mod ops_handlers_test;

use crate::backup_executor::{run_backup, BackupExecutorState};
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::Role;

/// Same v1 stopgap as `retention-service`'s `/v1/sweep` (ADR-0011 point 5): a service-to-service
/// operational trigger with no end user behind the call, gated on the shared internal secret
/// rather than a role, since there's no session/tenant identity at this call site.
fn has_valid_internal_secret(state: &AppState, headers: &HeaderMap) -> bool {
    let provided = headers.get("x-internal-secret").and_then(|v| v.to_str().ok());
    provided == Some(state.internal_secret.as_str())
}

/// POST /v1/backup/run — triggers one backup pass. Externally scheduled (`backup-scheduler` in
/// `docker-compose.yml`, mirroring `retention-sweep-scheduler`) rather than an in-process timer,
/// so this is the only way a backup runs (ADR-0055).
pub async fn trigger_backup(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !has_valid_internal_secret(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }

    let executor_state = BackupExecutorState {
        run_repository: state.run_repository.clone(),
        store: state.store.clone(),
        dump_runner: state.dump_runner.clone(),
    };
    let outcome = run_backup(&executor_state, chrono::Utc::now()).await;
    Json(outcome).into_response()
}

/// Backups are platform-wide (a whole-database dump, not scoped to one tenant), so unlike every
/// tenant-scoped Console UI read path there's no `X-Tenant-Id` to check here -- only that the
/// caller is both the Console UI (internal secret) and an admin session (`X-Role`), matching the
/// access bar `/v1/users` and Active Sessions use for other ops-sensitive views.
fn require_admin(headers: &HeaderMap) -> Option<Response> {
    let raw = match headers.get("x-role").and_then(|v| v.to_str().ok()) {
        Some(raw) => raw,
        None => return Some((StatusCode::UNAUTHORIZED, "missing X-Role header").into_response()),
    };
    match raw.parse::<Role>() {
        Ok(role) if role.at_least(Role::Admin) => None,
        Ok(_) => Some(
            (StatusCode::FORBIDDEN, "role does not have permission to view backup status")
                .into_response(),
        ),
        Err(_) => {
            Some((StatusCode::BAD_REQUEST, "X-Role is not a recognized role").into_response())
        }
    }
}

const DEFAULT_STATUS_LIMIT: i64 = 20;

/// GET /v1/backup/status — the last N backup runs, most recent first, for the Console UI's
/// Backups page (ADR-0055).
pub async fn get_backup_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !has_valid_internal_secret(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match state.run_repository.list_recent(DEFAULT_STATUS_LIMIT).await {
        Ok(runs) => Json(runs).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
