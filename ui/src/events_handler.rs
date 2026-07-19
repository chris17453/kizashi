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
        Ok(result) => Html(
            EventsTemplate {
                show_nav: true,
                events: result.events,
                chart_bars,
                page,
                has_more: result.has_more,
                error: None,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            EventsTemplate {
                show_nav: true,
                events: vec![],
                chart_bars,
                page,
                has_more: false,
                error: Some(e.to_string()),
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}
