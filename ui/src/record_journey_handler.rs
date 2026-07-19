#[path = "record_journey_handler_test.rs"]
#[cfg(test)]
mod record_journey_handler_test;

use crate::session_guard::require_session;
use crate::{ActionExecutionSummary, AppState, EventSummary, RecordSummary};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

/// One Event this record contributed to, plus every action execution it caused — a single
/// link/hop in the journey view.
struct EventLink {
    event: EventSummary,
    executions: Vec<ActionExecutionSummary>,
}

#[derive(Template)]
#[template(path = "record_journey.html")]
struct RecordJourneyTemplate {
    show_nav: bool,
    record: Option<RecordSummary>,
    event_links: Vec<EventLink>,
    error: Option<String>,
}

/// GET /data/:id/journey — the record→event→action lineage view (ADR-0017): given a raw
/// record, shows every Event it contributed to (via `record_ids`) and every ActionExecution
/// each of those Events caused. A Palantir-style link/investigative view built entirely from
/// existing endpoints — `GET /data/:id`, `GET /v1/events?record_id=`, and Action Executor's
/// `GET /v1/action-executions?event_id=` — with no new backend query added here.
pub async fn get_record_journey(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let record = match state.stats_client.get_record(session.tenant_id, id).await {
        Ok(record) => record,
        Err(e) => {
            return Html(
                RecordJourneyTemplate {
                    show_nav: true,
                    record: None,
                    event_links: vec![],
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let events = match state.events_client.list_events_for_record(&session.bearer_token, id).await {
        Ok(events) => events,
        Err(e) => {
            return Html(
                RecordJourneyTemplate {
                    show_nav: true,
                    record,
                    event_links: vec![],
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let mut event_links = Vec::with_capacity(events.len());
    for event in events {
        let executions = state
            .execution_client
            .list_executions_for_event(session.tenant_id, event.id)
            .await
            .unwrap_or_default();
        event_links.push(EventLink { event, executions });
    }

    Html(
        RecordJourneyTemplate { show_nav: true, record, event_links, error: None }
            .render()
            .unwrap(),
    )
    .into_response()
}
