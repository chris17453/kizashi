#[path = "recent_audit_log_handler_test.rs"]
#[cfg(test)]
mod recent_audit_log_handler_test;

use crate::audit_log_client::{AuditLogClient, AuditLogEntry};
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use std::sync::Arc;

const PAGE_SIZE: u32 = 50;

struct RecentAuditLogEntryView {
    service: &'static str,
    change_type: String,
    entity_type: String,
    actor: String,
    changed_at: DateTime<Utc>,
}

#[derive(serde::Deserialize, Default)]
pub struct RecentAuditLogQuery {
    before: Option<DateTime<Utc>>,
    #[serde(default)]
    q: String,
}

/// Case-insensitive substring match across actor/entity_type/change_type -- same in-handler
/// filter shape as the other list-page searches (ADR-0062), but since this page is already
/// cursor-paginated (`before`), it only searches the *currently fetched* page, not the
/// tenant's full audit history -- the same accepted "doesn't compose with pagination in one
/// request" limitation ADR-0063 documented for Login Attempts.
fn matches_query(entry: &RecentAuditLogEntryView, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    entry.actor.to_lowercase().contains(&q)
        || entry.entity_type.to_lowercase().contains(&q)
        || entry.change_type.to_lowercase().contains(&q)
}

#[derive(Template)]
#[template(path = "recent_audit_log.html")]
struct RecentAuditLogTemplate {
    show_nav: bool,
    entries: Vec<RecentAuditLogEntryView>,
    next_before: Option<DateTime<Utc>>,
    error: Option<String>,
    q: String,
}

/// Fetches one page (up to `limit` per source, `PAGE_SIZE`-capped by the caller) from all three
/// audit sources, merged and sorted most-recent-first -- the shared core of both the HTML page
/// and the CSV export (ADR-0049), so the two can never silently diverge in what counts as "the
/// tenant's recent activity."
async fn fetch_merged_page(
    state: &AppState,
    tenant_id: uuid::Uuid,
    before: Option<DateTime<Utc>>,
    limit: u32,
) -> (Vec<(&'static str, AuditLogEntry)>, Vec<String>) {
    let sources: [(&'static str, &Arc<dyn AuditLogClient>); 3] = [
        ("config-admin-service", &state.config_audit_log_client),
        ("retention-service", &state.retention_audit_log_client),
        ("auth-service", &state.auth_audit_log_client),
    ];

    let mut merged: Vec<(&'static str, AuditLogEntry)> = Vec::new();
    let mut errors = Vec::new();
    for (label, client) in sources {
        match client.list_recent(tenant_id, limit, before).await {
            Ok(entries) => merged.extend(entries.into_iter().map(|e| (label, e))),
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
    merged.sort_by_key(|(_, e)| std::cmp::Reverse(e.changed_at));
    (merged, errors)
}

/// GET /audit-log — a single, browsable, filterable-by-nothing-required activity feed merging
/// every tenant-scoped audit trail (config-admin-service's triggers/mappings/agents/analysis
/// config, retention-service's retention policies, auth-service's users/branding), most-recent
/// first (ADR-0045). Unlike `/audit-log/:service/:entity_id`, this needs no prior knowledge of
/// which entity to inspect -- the baseline enterprise-compliance expectation of "show me every
/// admin action recently," not just "show me this one record's history."
pub async fn get_recent_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RecentAuditLogQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let (mut merged, errors) =
        fetch_merged_page(&state, session.tenant_id, query.before, PAGE_SIZE).await;
    merged.truncate(PAGE_SIZE as usize);

    // Cursor computed from the full fetched page, before the search filter narrows what's
    // displayed -- "Load older" must keep advancing through real history regardless of what q
    // currently matches.
    let next_before = merged.last().map(|(_, e)| e.changed_at);
    let entries: Vec<RecentAuditLogEntryView> = merged
        .into_iter()
        .map(|(service, e)| RecentAuditLogEntryView {
            service,
            change_type: e.change_type,
            entity_type: e.entity_type,
            actor: e.actor,
            changed_at: e.changed_at,
        })
        .filter(|entry| matches_query(entry, &query.q))
        .collect();
    let error = if errors.is_empty() { None } else { Some(errors.join("; ")) };

    Html(
        RecentAuditLogTemplate { show_nav: true, entries, next_before, error, q: query.q }
            .render()
            .unwrap(),
    )
    .into_response()
}

/// Successive pages fetched per audit source when building the CSV export, each page requesting
/// the backend's own maximum (200, config-admin-service/auth-service/retention-service all cap
/// `list_recent` there) -- bounds the export to `CSV_MAX_PAGES * 200 * 3` rows worst case
/// (6000) rather than looping until each source is exhausted, since an unbounded export against
/// a very long-lived tenant could otherwise take an unreasonable time/response size.
const CSV_MAX_PAGES: usize = 10;
const CSV_PAGE_LIMIT: u32 = 200;

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[derive(serde::Deserialize)]
pub struct CsvExportQuery {
    before: Option<DateTime<Utc>>,
}

/// GET /audit-log/export.csv — a compliance-report export of the same merged feed
/// `get_recent_audit_log` shows, paginated internally (not by the caller within one request) up
/// to `CSV_MAX_PAGES` pages per source so a single request can produce a genuinely useful export
/// instead of just the one page the HTML view shows (ADR-0049). Accepts the same `?before=`
/// cursor the HTML page's "Load older" link uses, so a tenant with more than `CSV_MAX_PAGES *
/// CSV_PAGE_LIMIT * 3` (6000) rows of history isn't capped at the first page forever -- a
/// second export request with `?before=<the last row's changed_at>` continues where the first
/// left off, same cursor semantics as `list_recent` itself (ADR-0058 follow-up).
pub async fn get_recent_audit_log_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CsvExportQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let mut all_rows: Vec<(&'static str, AuditLogEntry)> = Vec::new();
    let mut before = query.before;
    let mut exhausted = false;
    for _ in 0..CSV_MAX_PAGES {
        let (page, _errors) =
            fetch_merged_page(&state, session.tenant_id, before, CSV_PAGE_LIMIT).await;
        if page.is_empty() {
            exhausted = true;
            break;
        }
        before = page.last().map(|(_, e)| e.changed_at);
        all_rows.extend(page);
    }
    all_rows.sort_by_key(|(_, e)| std::cmp::Reverse(e.changed_at));

    let mut csv = String::from("changed_at,service,entity_type,change_type,actor\n");
    for (service, entry) in &all_rows {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            entry.changed_at.to_rfc3339(),
            csv_escape(service),
            csv_escape(&entry.entity_type),
            csv_escape(&entry.change_type),
            csv_escape(&entry.actor),
        ));
    }

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"audit-log-{}.csv\"", session.tenant_id).parse().unwrap(),
    );
    // Not `exhausted` means the loop ran out of iterations while at least one source still had
    // more rows to give -- there may be more history beyond what this export contains, so
    // surface the continuation cursor rather than silently truncating (CLAUDE.md's "no silent
    // caps").
    if !exhausted {
        if let Some(next_before) = before {
            headers.insert(
                axum::http::HeaderName::from_static("x-next-before"),
                next_before.to_rfc3339().parse().unwrap(),
            );
        }
    }

    (headers, csv).into_response()
}
