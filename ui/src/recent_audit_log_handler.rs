#[path = "recent_audit_log_handler_test.rs"]
#[cfg(test)]
mod recent_audit_log_handler_test;

use crate::audit_log_client::{AuditLogClient, AuditLogEntry};
use crate::ontology_client;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::sync::Arc;

const PAGE_SIZE: u32 = 50;
const FILTER_MAX_PAGES: usize = 10;

struct RecentAuditLogEntryView {
    id: uuid::Uuid,
    entity_id: uuid::Uuid,
    service: &'static str,
    change_type: String,
    entity_type: String,
    actor: String,
    changed_at: DateTime<Utc>,
    before: String,
    after: String,
    entity_href: String,
}

fn entity_audit_href(service: &str, entity_id: uuid::Uuid) -> String {
    let route = match service {
        "config-admin-service" => "config",
        "retention-service" => "retention",
        "auth-service" => "auth",
        "incident-service" => "incident",
        "ontology-service" => "ontology",
        _ => return String::new(),
    };
    format!("/audit-log/{route}/{entity_id}")
}

struct AuditPostureRow {
    label: String,
    count: usize,
    percentage: usize,
}

struct AuditTimelineBar {
    date: String,
    total: usize,
    height_pct: usize,
    config: usize,
    retention: usize,
    auth: usize,
    incident: usize,
    ontology: usize,
}

fn audit_timeline(entries: &[RecentAuditLogEntryView]) -> Vec<AuditTimelineBar> {
    let mut buckets = BTreeMap::<String, [usize; 5]>::new();
    for entry in entries {
        let counts = buckets.entry(entry.changed_at.format("%Y-%m-%d").to_string()).or_default();
        let index = match entry.service {
            "config-admin-service" => 0,
            "retention-service" => 1,
            "auth-service" => 2,
            "incident-service" => 3,
            "ontology-service" => 4,
            _ => continue,
        };
        counts[index] += 1;
    }
    let max = buckets.values().map(|counts| counts.iter().sum::<usize>()).max().unwrap_or(1);
    buckets
        .into_iter()
        .map(|(date, counts)| {
            let total = counts.iter().sum::<usize>();
            AuditTimelineBar {
                date,
                total,
                height_pct: (total * 100 / max).max(8),
                config: counts[0],
                retention: counts[1],
                auth: counts[2],
                incident: counts[3],
                ontology: counts[4],
            }
        })
        .collect()
}

fn audit_json(value: Option<&serde_json::Value>) -> String {
    value
        .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()))
        .unwrap_or_else(|| "(none)".to_string())
}

#[derive(serde::Deserialize, Default)]
pub struct RecentAuditLogQuery {
    before: Option<DateTime<Utc>>,
    #[serde(default)]
    q: String,
    #[serde(default)]
    service: String,
    #[serde(default)]
    change_type: String,
    #[serde(default)]
    date: String,
}

/// Case-insensitive substring match across actor/entity_type/change_type. Filtered requests walk
/// bounded cursor pages in the handler so a match is not silently missed just because it falls
/// behind the first page of a busy tenant's audit feed.
fn matches_query(
    entry: &RecentAuditLogEntryView,
    q: &str,
    service: &str,
    change_type: &str,
    date: &str,
) -> bool {
    matches_fields(
        entry.service,
        &entry.entity_type,
        &entry.change_type,
        &entry.actor,
        entry.id,
        entry.entity_id,
        entry.changed_at,
        q,
        service,
        change_type,
        date,
    )
}

fn matches_fields(
    entry_service: &str,
    entity_type: &str,
    entry_change_type: &str,
    actor: &str,
    id: uuid::Uuid,
    entity_id: uuid::Uuid,
    changed_at: DateTime<Utc>,
    q: &str,
    service: &str,
    change_type: &str,
    date: &str,
) -> bool {
    let service_match = service.is_empty() || entry_service == service;
    let change_match =
        change_type.is_empty() || entry_change_type.eq_ignore_ascii_case(change_type);
    if !service_match || !change_match {
        return false;
    }
    if !date.is_empty() && changed_at.format("%Y-%m-%d").to_string() != date {
        return false;
    }
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    actor.to_lowercase().contains(&q)
        || entity_type.to_lowercase().contains(&q)
        || entry_change_type.to_lowercase().contains(&q)
        || id.to_string().contains(&q)
        || entity_id.to_string().contains(&q)
}

#[derive(Template)]
#[template(path = "recent_audit_log.html")]
struct RecentAuditLogTemplate {
    show_nav: bool,
    is_admin: bool,
    entries: Vec<RecentAuditLogEntryView>,
    next_before: Option<DateTime<Utc>>,
    error: Option<String>,
    q: String,
    service: String,
    change_type: String,
    date: String,
    service_posture: Vec<AuditPostureRow>,
    change_posture: Vec<AuditPostureRow>,
    timeline: Vec<AuditTimelineBar>,
}

fn posture_rows<I>(values: I, total: usize) -> Vec<AuditPostureRow>
where
    I: IntoIterator<Item = String>,
{
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    let mut rows: Vec<_> = counts
        .into_iter()
        .map(|(label, count)| AuditPostureRow {
            label,
            count,
            percentage: if total == 0 { 0 } else { count * 100 / total },
        })
        .collect();
    rows.sort_by(|left, right| {
        right.count.cmp(&left.count).then_with(|| left.label.cmp(&right.label))
    });
    rows.truncate(6);
    rows
}

/// Fetches one page (up to `limit` per source, `PAGE_SIZE`-capped by the caller) from all three
/// audit sources, merged and sorted most-recent-first -- the shared core of both the HTML page
/// and the CSV export (ADR-0049), so the two can never silently diverge in what counts as "the
/// tenant's recent activity."
pub(crate) async fn fetch_merged_page(
    state: &AppState,
    tenant_id: uuid::Uuid,
    bearer_token: &str,
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
    match state.incidents_client.list_recent_audit_log(tenant_id, limit, before).await {
        Ok(entries) => merged.extend(entries.into_iter().map(|e| ("incident-service", e))),
        Err(e) => errors.push(format!("incident-service: {e}")),
    }
    if let Some(client) = ontology_client::global() {
        match client.list_action_invocations(bearer_token).await {
            Ok(invocations) => {
                let mut entries: Vec<AuditLogEntry> = invocations
                    .into_iter()
                    .map(|invocation| {
                        let entity_id = invocation
                            .target_object_ids
                            .as_array()
                            .and_then(|targets| targets.first())
                            .and_then(|target| target.as_str())
                            .and_then(|target| target.parse().ok())
                            .unwrap_or(invocation.action_type_id);
                        let actor = invocation
                            .triggering_event_ref
                            .get("actor")
                            .and_then(|value| value.as_str())
                            .unwrap_or("system")
                            .to_string();
                        AuditLogEntry {
                            id: invocation.id,
                            entity_type: "governed_action_invocation".to_string(),
                            entity_id,
                            change_type: "invoked".to_string(),
                            actor,
                            before: None,
                            after: serde_json::json!({
                                "action_type_id": invocation.action_type_id,
                                "target_object_ids": invocation.target_object_ids,
                                "parameters": invocation.parameters,
                                "outcome": invocation.outcome,
                                "triggering_event_ref": invocation.triggering_event_ref,
                            }),
                            changed_at: invocation.executed_at,
                        }
                    })
                    .filter(|entry| before.map_or(true, |cursor| entry.changed_at < cursor))
                    .collect();
                entries.sort_by_key(|entry| std::cmp::Reverse(entry.changed_at));
                entries.truncate(limit as usize);
                merged.extend(entries.into_iter().map(|entry| ("ontology-service", entry)));
            }
            Err(e) => errors.push(format!("ontology-service: {e}")),
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
    let is_admin = session.role.at_least(common::Role::Admin);

    let filtering = !query.q.is_empty()
        || !query.service.is_empty()
        || !query.change_type.is_empty()
        || !query.date.is_empty();
    let pages = if filtering { FILTER_MAX_PAGES } else { 1 };
    let mut cursor = query.before;
    let mut next_before = None;
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for _ in 0..pages {
        let (mut merged, page_errors) =
            fetch_merged_page(&state, session.tenant_id, &session.bearer_token, cursor, PAGE_SIZE)
                .await;
        errors.extend(page_errors);
        merged.truncate(PAGE_SIZE as usize);
        let page_cursor = merged.last().map(|(_, e)| e.changed_at);
        next_before = page_cursor;
        entries.extend(
            merged
                .into_iter()
                .map(|(service, e)| RecentAuditLogEntryView {
                    id: e.id,
                    entity_id: e.entity_id,
                    service,
                    change_type: e.change_type,
                    entity_type: e.entity_type,
                    actor: e.actor,
                    changed_at: e.changed_at,
                    before: audit_json(e.before.as_ref()),
                    after: audit_json(Some(&e.after)),
                    entity_href: entity_audit_href(service, e.entity_id),
                })
                .filter(|entry| {
                    matches_query(entry, &query.q, &query.service, &query.change_type, &query.date)
                }),
        );
        if entries.len() >= PAGE_SIZE as usize || page_cursor.is_none() {
            break;
        }
        cursor = page_cursor;
    }
    entries.truncate(PAGE_SIZE as usize);
    let error = if errors.is_empty() { None } else { Some(errors.join("; ")) };
    let service_posture =
        posture_rows(entries.iter().map(|entry| entry.service.to_string()), entries.len());
    let change_posture =
        posture_rows(entries.iter().map(|entry| entry.change_type.clone()), entries.len());
    let timeline = audit_timeline(&entries);

    Html(
        RecentAuditLogTemplate {
            show_nav: true,
            is_admin,
            entries,
            next_before,
            error,
            q: query.q,
            service: query.service,
            change_type: query.change_type,
            date: query.date,
            service_posture,
            change_posture,
            timeline,
        }
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
    #[serde(default)]
    before: Option<DateTime<Utc>>,
    #[serde(default)]
    q: String,
    #[serde(default)]
    service: String,
    #[serde(default)]
    change_type: String,
    #[serde(default)]
    date: String,
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
        let (page, _errors) = fetch_merged_page(
            &state,
            session.tenant_id,
            &session.bearer_token,
            before,
            CSV_PAGE_LIMIT,
        )
        .await;
        if page.is_empty() {
            exhausted = true;
            break;
        }
        before = page.last().map(|(_, e)| e.changed_at);
        all_rows.extend(page.into_iter().filter(|(service, entry)| {
            matches_fields(
                service,
                &entry.entity_type,
                &entry.change_type,
                &entry.actor,
                entry.id,
                entry.entity_id,
                entry.changed_at,
                &query.q,
                &query.service,
                &query.change_type,
                &query.date,
            )
        }));
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
