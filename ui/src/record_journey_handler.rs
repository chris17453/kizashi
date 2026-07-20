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
/// link/hop in the journey view. `latency_from_ingest` and each execution's latency are
/// pre-formatted here (not computed in the Askama template, which can't do date arithmetic) —
/// the waterfall-style timing view (how long each pipeline hop actually took) that the plain
/// lineage box diagram didn't show before.
struct EventLink {
    event: EventSummary,
    latency_from_ingest: Option<String>,
    executions: Vec<ExecutionLink>,
}

struct ExecutionLink {
    execution: ActionExecutionSummary,
    latency_from_event: String,
}

/// Formats a (possibly negative, e.g. clock skew) duration as a short human-readable string —
/// `"1.2s"`, `"450ms"`, `"2m 3s"`. Never panics on a negative delta; reports it as `"0ms"`
/// rather than a confusing negative number, since a negative pipeline hop duration is always a
/// clock-skew artifact, not a real "time travel" the operator should have to interpret.
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

#[derive(Template)]
#[template(path = "record_journey.html")]
struct RecordJourneyTemplate {
    show_nav: bool,
    is_admin: bool,
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
    let is_admin = session.role.at_least(common::Role::Admin);

    let record = match state.stats_client.get_record(session.tenant_id, id).await {
        Ok(record) => record,
        Err(e) => {
            return Html(
                RecordJourneyTemplate {
                    show_nav: true,
                    is_admin,
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
                    is_admin,
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
            .unwrap_or_default()
            .into_iter()
            .map(|execution| ExecutionLink {
                latency_from_event: format_latency(event.occurred_at, execution.executed_at),
                execution,
            })
            .collect();
        let latency_from_ingest =
            record.as_ref().map(|r| format_latency(r.ingested_at, event.occurred_at));
        event_links.push(EventLink { event, latency_from_ingest, executions });
    }

    Html(
        RecordJourneyTemplate { show_nav: true, is_admin, record, event_links, error: None }
            .render()
            .unwrap(),
    )
    .into_response()
}
