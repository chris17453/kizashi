#[path = "event_detail_handler_test.rs"]
#[cfg(test)]
mod event_detail_handler_test;

use crate::events_client::EventDetail;
use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

/// Formats a (possibly negative, e.g. clock skew) duration as a short human-readable string --
/// same helper `record_journey_handler` already uses, duplicated here rather than shared per
/// this codebase's convention of small page-local helpers over a shared crate-wide module.
fn format_latency(
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> String {
    let millis = (to - from).num_milliseconds().max(0);
    if millis < 1000 {
        format!("{millis}ms")
    } else if millis < 60_000 {
        format!("{:.1}s", millis as f64 / 1000.0)
    } else {
        format!("{}m {}s", millis / 60_000, (millis % 60_000) / 1000)
    }
}

/// One timeline entry -- either the event firing itself or a subsequent action execution it
/// caused, pre-sorted chronologically so the template just renders top-to-bottom.
struct TimelineEntry {
    label: String,
    detail: String,
    at: chrono::DateTime<chrono::Utc>,
    latency_from_event: Option<String>,
    is_failure: bool,
}

#[derive(Template)]
#[template(path = "event_detail.html")]
struct EventDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    event: Option<EventDetail>,
    status_class: String,
    payload_pretty: String,
    contributing_records: Vec<RecordSummary>,
    timeline: Vec<TimelineEntry>,
    error: Option<String>,
}

/// Askama templates can't call arbitrary Rust functions, so the status-to-CSS-class mapping is
/// computed here rather than as an inline conditional in the template.
fn status_class(status: &str) -> String {
    match status {
        "actioned" => "up",
        "dismissed" => "down",
        _ => "open",
    }
    .to_string()
}

fn error_page(is_admin: bool, message: String) -> Response {
    Html(
        EventDetailTemplate {
            show_nav: true,
            is_admin,
            event: None,
            status_class: String::new(),
            payload_pretty: String::new(),
            contributing_records: vec![],
            timeline: vec![],
            error: Some(message),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

/// GET /events/:id — the investigation-focused counterpart to the flat Events table: what this
/// event actually was (full payload, entity_ref, source connectors), which raw records
/// contributed to it, and a chronological timeline of everything that happened because of it
/// (each action execution, with latency from the event firing). Closest analog to an incident
/// tool's "alert detail" panel -- previously an operator investigating one event had no page to
/// land on beyond the flat table row.
pub async fn get_event_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);

    let event = match state.events_client.get_event(&session.bearer_token, id).await {
        Ok(Some(event)) => event,
        Ok(None) => return error_page(is_admin, "no event found with this id".to_string()),
        Err(e) => return error_page(is_admin, e.to_string()),
    };

    let payload_pretty =
        serde_json::to_string_pretty(&event.payload).unwrap_or_else(|_| event.payload.to_string());

    let mut contributing_records = Vec::with_capacity(event.record_ids.len());
    for record_id in &event.record_ids {
        if let Ok(Some(record)) = state.stats_client.get_record(session.tenant_id, *record_id).await
        {
            contributing_records.push(record);
        }
    }

    let executions = state
        .execution_client
        .list_executions_for_event(session.tenant_id, event.id)
        .await
        .unwrap_or_default();

    let mut timeline = vec![TimelineEntry {
        label: "Event fired".to_string(),
        detail: event.event_type.clone(),
        at: event.occurred_at,
        latency_from_event: None,
        is_failure: false,
    }];
    for execution in &executions {
        timeline.push(TimelineEntry {
            label: format!("Action: {}", execution.action_type),
            detail: execution.status.clone(),
            at: execution.executed_at,
            latency_from_event: Some(format_latency(event.occurred_at, execution.executed_at)),
            is_failure: execution.status != "sent",
        });
    }
    timeline.sort_by_key(|entry| entry.at);
    let status_class = status_class(&event.status);

    Html(
        EventDetailTemplate {
            show_nav: true,
            is_admin,
            event: Some(event),
            status_class,
            payload_pretty,
            contributing_records,
            timeline,
            error: None,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
