#[path = "sessions_handler_test.rs"]
#[cfg(test)]
mod sessions_handler_test;

use crate::session_guard::{require_session, session_cookie_value};
use crate::AppState;
use askama::Template;
use axum::extract::{Path, Query, State};
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

#[derive(serde::Deserialize, Default)]
pub struct SessionsQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    age: String,
}

/// Case-insensitive substring match on username -- same in-handler-filter shape as the Users
/// and API Keys pages (ADR-0062): the session store's `list_for_tenant` has no search parameter
/// of its own, and a tenant's active-session count is realistically small enough to filter
/// after the fetch.
fn matches_query(row: &SessionRow, q: &str, role: &str, age: &str, now: DateTime<Utc>) -> bool {
    (q.is_empty() || row.username.to_lowercase().contains(&q.to_lowercase()))
        && (role.is_empty() || row.role_str == role)
        && (age.is_empty()
            || (age == "recent" && row.created_at >= now - chrono::Duration::hours(24))
            || (age == "stale" && row.created_at < now - chrono::Duration::days(30)))
}

/// Same shape as the Users page's sortable columns (ADR-0064), applied after the search filter
/// so search and sort compose. Unset `sort` keeps this page's original default -- most
/// recently signed-in first -- rather than switching to ascending-by-username like Users, since
/// "who signed in most recently" is the more useful default for a security-review page.
fn sort_rows(rows: &mut [SessionRow], sort: &str, dir: &str) {
    match sort {
        "username" => rows.sort_by_key(|s| s.username.to_lowercase()),
        "role" => rows.sort_by_key(|s| s.role_str.clone()),
        _ => {
            rows.sort_by_key(|s| std::cmp::Reverse(s.created_at));
            if dir == "asc" {
                rows.reverse();
            }
            return;
        }
    }
    if dir == "desc" {
        rows.reverse();
    }
}

#[derive(Template)]
#[template(path = "sessions.html")]
struct SessionsTemplate {
    show_nav: bool,
    is_admin: bool,
    sessions: Vec<SessionRow>,
    q: String,
    role: String,
    sort: String,
    dir: String,
    age: String,
    admin_count: usize,
    operator_count: usize,
    viewer_count: usize,
    recent_count: usize,
    stale_count: usize,
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
pub async fn get_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SessionsQuery>,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(Role::Admin);
    let current_id = session_cookie_value(&headers);

    let now = Utc::now();
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
        .filter(|row| matches_query(row, &query.q, &query.role, &query.age, now))
        .collect();
    sort_rows(&mut sessions, &query.sort, &query.dir);
    let admin_count = sessions.iter().filter(|row| row.role_str == "admin").count();
    let operator_count = sessions.iter().filter(|row| row.role_str == "operator").count();
    let viewer_count = sessions.iter().filter(|row| row.role_str == "viewer").count();
    let recent_count =
        sessions.iter().filter(|row| row.created_at >= now - chrono::Duration::hours(24)).count();
    let stale_count =
        sessions.iter().filter(|row| row.created_at < now - chrono::Duration::days(30)).count();

    Html(
        SessionsTemplate {
            show_nav: true,
            is_admin,
            sessions,
            q: query.q,
            role: query.role,
            sort: query.sort,
            dir: query.dir,
            age: query.age,
            admin_count,
            operator_count,
            viewer_count,
            recent_count,
            stale_count,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

/// Best-effort: session revocation itself has already succeeded by the time this is called, and
/// the two systems (Console UI's in-memory session store, Auth Service's Postgres-backed audit
/// log) are different processes with no shared transaction -- a failure to *record* the
/// revocation must never undo or block the revocation itself (ADR-0101), same "log even if the
/// audit write fails" philosophy as `record_attempt` in auth-service's own login handler.
/// Silently skipped if the session id doesn't parse as a `Uuid` (auth-service's audit entity_id
/// is `Uuid`-typed), which should never happen since `InMemorySessionStore::create` always mints
/// one, but a malformed id must not panic this best-effort call.
async fn record_session_revocation_audit(
    state: &AppState,
    session: &crate::Session,
    revoked_session_id: &str,
    revoked_username: &str,
) {
    let Ok(session_id) = revoked_session_id.parse() else {
        return;
    };
    let _ = state
        .users_client
        .record_session_revocation(
            session.tenant_id,
            session.role,
            &session.username,
            session_id,
            revoked_username,
        )
        .await;
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

    let target = state
        .session_store
        .list_for_tenant(session.tenant_id)
        .await
        .into_iter()
        .find(|(session_id, _)| *session_id == id);
    if let Some((_, target_session)) = target {
        state.session_store.delete(&id).await;
        record_session_revocation_audit(&state, &session, &id, &target_session.username).await;
    }

    Redirect::to("/security/sessions").into_response()
}

/// `axum::extract::Form` deserializes via `serde_urlencoded`, which -- unlike some other form
/// crates -- does NOT collect repeated same-named fields (one checkbox per row, all named
/// `ids`) into a `Vec`; it only supports flat scalar struct fields. Parsing the raw body as a
/// flat list of `(key, value)` pairs instead and filtering for `"ids"` sidesteps that
/// limitation. Same pattern as API Keys' `post_bulk_revoke_api_keys` and Users'
/// `post_bulk_delete_users` (ADR-0065/ADR-0096) -- session ids are opaque strings, not `Uuid`s,
/// so unlike those this doesn't parse each value further.
fn parse_ids(raw_body: &[u8]) -> Vec<String> {
    let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body) else {
        return Vec::new();
    };
    pairs.into_iter().filter(|(key, _)| key == "ids").map(|(_, value)| value).collect()
}

/// POST /security/sessions/bulk-revoke — force-terminates every selected session (same
/// bulk-action pattern API Keys, Sensors, Users, and Retention Policies already have,
/// ADR-0065/ADR-0095/ADR-0096: loop over the existing single-item revoke rather than a new bulk
/// backend endpoint). Same tenant-membership check as `post_revoke_session` applied per id, so
/// an admin can't blind-guess another tenant's session id into the bulk form.
pub async fn post_bulk_revoke_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_admin_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let tenant_sessions: std::collections::HashMap<String, crate::Session> =
        state.session_store.list_for_tenant(session.tenant_id).await.into_iter().collect();

    for id in parse_ids(&body) {
        if let Some(target_session) = tenant_sessions.get(&id) {
            state.session_store.delete(&id).await;
            record_session_revocation_audit(&state, &session, &id, &target_session.username).await;
        }
    }

    Redirect::to("/security/sessions").into_response()
}
