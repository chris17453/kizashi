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

fn default_page() -> i64 {
    0
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
    events: Vec<EventSummary>,
    chart_bars: Vec<ChartBar>,
    page: i64,
    has_more: bool,
    error: Option<String>,
    q: String,
    sort: String,
    dir: String,
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

    let page = query.page.max(0);
    let offset = (page * DEFAULT_PAGE_SIZE) as u32;

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
        .list_events(&session.bearer_token, DEFAULT_PAGE_SIZE as u32, offset)
        .await
    {
        Ok(result) => {
            let mut events: Vec<EventSummary> =
                result.events.into_iter().filter(|e| matches_query(e, &query.q)).collect();
            sort_rows(&mut events, &query.sort, &query.dir);
            Html(
                EventsTemplate {
                    show_nav: true,
                    events,
                    chart_bars,
                    page,
                    has_more: result.has_more,
                    error: None,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            EventsTemplate {
                show_nav: true,
                events: vec![],
                chart_bars,
                page,
                has_more: false,
                error: Some(e.to_string()),
                q: query.q,
                sort: query.sort,
                dir: query.dir,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
