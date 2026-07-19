#[path = "overview_handler_test.rs"]
#[cfg(test)]
mod overview_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

#[derive(Template)]
#[template(path = "overview.html")]
struct OverviewTemplate {
    show_nav: bool,
    agent_count: usize,
    active_agent_count: usize,
    total_records: i64,
    event_count: usize,
    platform_status: String,
    services_up: usize,
    services_total: usize,
}

/// GET /overview — the landing dashboard: KPI cards summarizing agents, ingestion volume,
/// events, and platform health at a glance, each pulled from the same backends every other
/// page already reads (no new data path — just presented as tiles instead of a table).
pub async fn get_overview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let agents = state.agents_client.list_agents(session.tenant_id).await.unwrap_or_default();
    let connector_stats =
        state.stats_client.connector_stats(session.tenant_id).await.unwrap_or_default();
    // Capped at 1000 (the same ceiling the backend itself clamps to) — a KPI tile approximates
    // at very high volume rather than needing an exact count, same tradeoff this dashboard
    // already made before pagination existed (it used to silently cap at the default limit).
    let events = state
        .events_client
        .list_events(&session.bearer_token, 1000, 0)
        .await
        .map(|page| page.events)
        .unwrap_or_default();
    let health = state.health_client.platform_health().await.ok();

    let active_connector_ids: std::collections::HashSet<&str> =
        connector_stats.iter().map(|s| s.connector_id.as_str()).collect();
    let active_agent_count =
        agents.iter().filter(|a| active_connector_ids.contains(a.name.as_str())).count();
    let total_records: i64 = connector_stats.iter().map(|s| s.record_count).sum();

    let (platform_status, services_up, services_total) = match &health {
        Some(h) => {
            let up = h.services.iter().filter(|s| s.status == "up").count();
            (h.status.clone(), up, h.services.len())
        }
        None => ("unknown".to_string(), 0, 0),
    };

    Html(
        OverviewTemplate {
            show_nav: true,
            agent_count: agents.len(),
            active_agent_count,
            total_records,
            event_count: events.len(),
            platform_status,
            services_up,
            services_total,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
