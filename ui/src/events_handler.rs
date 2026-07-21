#[path = "events_handler_filtering_test.rs"]
#[cfg(test)]
mod events_handler_filtering_test;
#[path = "events_handler_test.rs"]
#[cfg(test)]
mod events_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, EventSummary};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};

fn default_page() -> i64 {
    0
}

/// Parses `YYYY-MM-DD` date-only strings from `<input type="date">` into a fully inclusive
/// `DateTime<Utc>` range -- same shape as `data_handler`'s `parse_date_range`, duplicated here
/// rather than shared since it's a small, page-local concern (matching this codebase's existing
/// convention of per-handler `matches_query`/`sort_rows` helpers rather than a shared crate-wide
/// filter-parsing module).
fn parse_date_range(from: &str, to: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let parse_start = |s: &str| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    };
    let parse_end = |s: &str| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(23, 59, 59))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    };
    (parse_start(from), parse_end(to))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub dir: String,
    /// Scopes the search to a specific incident window -- forwarded to the backend, which
    /// already supported `since`/`until` (unlike `q`/`sort`, which only apply to the current
    /// fetched page since the backend has no substring-match/sort query of its own).
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
}

/// Case-insensitive substring match across event_type/group_key/status -- same shape as the
/// other list-page searches (ADR-0062). Like Triggers (ADR-0066), `list_events` is
/// server-paginated, so this only filters the *current page's* already-fetched events, not the
/// tenant's full event history.
fn matches_query(event: &EventSummary, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    event.event_type.to_lowercase().contains(&q)
        || event.group_key.to_lowercase().contains(&q)
        || event.status.to_lowercase().contains(&q)
}

/// Same shape as Triggers' sortable columns (ADR-0070), applied after the search filter and,
/// like search, only reordering the current page. `list_events` already returns most-recent
/// first, so an unset `sort` keeps that existing default.
fn sort_rows(rows: &mut [EventSummary], sort: &str, dir: &str) {
    match sort {
        "event_type" => rows.sort_by_key(|e| e.event_type.to_lowercase()),
        "group_key" => rows.sort_by_key(|e| e.group_key.to_lowercase()),
        "status" => rows.sort_by_key(|e| e.status.to_lowercase()),
        _ => {
            rows.sort_by_key(|e| std::cmp::Reverse(e.occurred_at));
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

/// One bar in the events-over-time chart — `height_pct` is pre-computed server-side (relative
/// to the busiest day in the window) so the template stays a plain inline SVG with no JS
/// (ADR-0014's no-JS-by-default stance).
struct ChartBar {
    date: String,
    count: u64,
    height_pct: u32,
}

fn build_chart_bars(counts: Vec<crate::events_client::DailyCount>) -> Vec<ChartBar> {
    let max = counts.iter().map(|c| c.count).max().unwrap_or(0).max(1);
    counts
        .into_iter()
        .map(|c| ChartBar {
            date: c.date,
            count: c.count,
            height_pct: ((c.count as f64 / max as f64) * 100.0).round() as u32,
        })
        .collect()
}

#[derive(Template)]
#[template(path = "events.html")]
struct EventsTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    events: Vec<EventSummary>,
    chart_bars: Vec<ChartBar>,
    page: i64,
    has_more: bool,
    error: Option<String>,
    q: String,
    sort: String,
    dir: String,
    from: String,
    to: String,
}

pub async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let page = query.page.max(0);
    let offset = (page * DEFAULT_PAGE_SIZE) as u32;
    let (filter_since, filter_until) = parse_date_range(&query.from, &query.to);

    // Independent of list_events below: a daily-counts failure shows an empty chart, not a
    // broken page — the table is the primary content, the chart is a supplementary view.
    let until = chrono::Utc::now();
    let since = until - chrono::Duration::days(30);
    let chart_bars = state
        .events_client
        .daily_counts(&session.bearer_token, since, until)
        .await
        .map(build_chart_bars)
        .unwrap_or_default();

    match state
        .events_client
        .list_events(
            &session.bearer_token,
            DEFAULT_PAGE_SIZE as u32,
            offset,
            filter_since,
            filter_until,
        )
        .await
    {
        Ok(result) => {
            let mut events: Vec<EventSummary> =
                result.events.into_iter().filter(|e| matches_query(e, &query.q)).collect();
            sort_rows(&mut events, &query.sort, &query.dir);
            Html(
                EventsTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    events,
                    chart_bars,
                    page,
                    has_more: result.has_more,
                    error: None,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                    from: query.from,
                    to: query.to,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            EventsTemplate {
                show_nav: true,
                is_admin,
                can_write,
                events: vec![],
                chart_bars,
                page,
                has_more: false,
                error: Some(e.to_string()),
                q: query.q,
                sort: query.sort,
                dir: query.dir,
                from: query.from,
                to: query.to,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

/// Successive pages fetched per export, each requesting `DEFAULT_PAGE_SIZE` rows -- bounds the
/// export to `CSV_MAX_PAGES * DEFAULT_PAGE_SIZE` rows worst case rather than looping until the
/// tenant's history is exhausted, same shape as `data_handler`'s and `login_attempts_handler`'s
/// CSV exports (ADR-0049).
const CSV_MAX_PAGES: usize = 10;

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// GET /events/export.csv — a compliance-export of the same feed `get_events` shows, honoring
/// the same `?from=`/`?to=` date-range filter, paginated internally up to `CSV_MAX_PAGES` pages
/// so a single request can produce a genuinely useful export rather than just the one page the
/// HTML view shows.
pub async fn get_events_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let (filter_since, filter_until) = parse_date_range(&query.from, &query.to);
    let mut all_events: Vec<EventSummary> = Vec::new();
    for i in 0..CSV_MAX_PAGES {
        let offset = (i as u32) * (DEFAULT_PAGE_SIZE as u32);
        let page = match state
            .events_client
            .list_events(
                &session.bearer_token,
                DEFAULT_PAGE_SIZE as u32,
                offset,
                filter_since,
                filter_until,
            )
            .await
        {
            Ok(page) => page,
            Err(e) => {
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                    .into_response()
            }
        };
        let has_more = page.has_more;
        all_events.extend(page.events);
        if !has_more {
            break;
        }
    }

    let mut csv = String::from("occurred_at,event_type,group_key,status\n");
    for event in &all_events {
        csv.push_str(&format!(
            "{},{},{},{}\n",
            event.occurred_at.to_rfc3339(),
            csv_escape(&event.event_type),
            csv_escape(&event.group_key),
            csv_escape(&event.status),
        ));
    }

    let mut resp_headers = axum::http::HeaderMap::new();
    resp_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    resp_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"events-{}.csv\"", session.tenant_id).parse().unwrap(),
    );

    (resp_headers, csv).into_response()
}
