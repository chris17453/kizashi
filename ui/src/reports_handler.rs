#[path = "reports_handler_test.rs"]
#[cfg(test)]
mod reports_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, ConnectorStatSummary};
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

struct EventTypeCount {
    event_type: String,
    count: usize,
}

/// GET /reports — the platform-wide aggregate view: ingestion volume per connector (from
/// Ingestion Service) alongside event counts per event type (from Query Gateway's event feed).
/// Both are cheap aggregates over data this platform already stores; nothing new is computed
/// or persisted for this page.
#[derive(Template)]
#[template(path = "reports.html")]
struct ReportsTemplate {
    show_nav: bool,
    connector_stats: Vec<ConnectorStatSummary>,
    event_counts: Vec<EventTypeCount>,
    error: Option<String>,
}

fn count_by_event_type(events: &[crate::EventSummary]) -> Vec<EventTypeCount> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for event in events {
        *counts.entry(event.event_type.clone()).or_insert(0) += 1;
    }
    counts.into_iter().map(|(event_type, count)| EventTypeCount { event_type, count }).collect()
}

pub async fn get_reports(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let connector_stats = match state.stats_client.connector_stats(session.tenant_id).await {
        Ok(stats) => stats,
        Err(e) => {
            return Html(
                ReportsTemplate {
                    show_nav: true,
                    connector_stats: vec![],
                    event_counts: vec![],
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let event_counts = match state.events_client.list_events(&session.bearer_token).await {
        Ok(events) => count_by_event_type(&events),
        Err(_) => vec![],
    };

    Html(
        ReportsTemplate { show_nav: true, connector_stats, event_counts, error: None }
            .render()
            .unwrap(),
    )
    .into_response()
}
