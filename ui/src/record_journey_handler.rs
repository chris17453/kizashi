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
    incident: Option<JourneyIncident>,
}

struct JourneyIncident {
    id: Uuid,
    title: String,
    severity: String,
    status: String,
}

struct ExecutionLink {
    execution: ActionExecutionSummary,
    latency_from_event: String,
}

struct JourneyTimingBar {
    label: String,
    detail: String,
    href: String,
    latency: String,
    width_pct: usize,
    failure: bool,
}

struct GovernedDecision {
    invocation_id: Uuid,
    action_name: String,
    target_id: Uuid,
    target_label: String,
    outcome: String,
    executed_at: chrono::DateTime<chrono::Utc>,
    review_status: String,
    review_assignee: Option<String>,
    review_stale: bool,
}

struct ModeledJourneyObject {
    id: Uuid,
    type_name: String,
    label: String,
}

struct JourneyObjectType {
    id: Uuid,
    name: String,
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
    source_record: Option<RecordSummary>,
    event_links: Vec<EventLink>,
    timing: Vec<JourneyTimingBar>,
    modeled_objects: Vec<ModeledJourneyObject>,
    object_types: Vec<JourneyObjectType>,
    can_write: bool,
    governed_decisions: Vec<GovernedDecision>,
    error: Option<String>,
    event_count: usize,
    execution_count: usize,
    incident_count: usize,
    modeled_object_count: usize,
    decision_count: usize,
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
                    source_record: None,
                    event_links: vec![],
                    timing: vec![],
                    modeled_objects: vec![],
                    object_types: vec![],
                    can_write: session.role.at_least(common::Role::Operator),
                    governed_decisions: vec![],
                    error: Some(e.to_string()),
                    event_count: 0,
                    execution_count: 0,
                    incident_count: 0,
                    modeled_object_count: 0,
                    decision_count: 0,
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
                    source_record: record,
                    event_links: vec![],
                    timing: vec![],
                    modeled_objects: vec![],
                    object_types: vec![],
                    can_write: session.role.at_least(common::Role::Operator),
                    governed_decisions: vec![],
                    error: Some(e.to_string()),
                    event_count: 0,
                    execution_count: 0,
                    incident_count: 0,
                    modeled_object_count: 0,
                    decision_count: 0,
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let incidents =
        state.incidents_client.list_incidents(session.tenant_id, None).await.unwrap_or_default();
    let (modeled_objects, governed_decisions, object_types) =
        if let Some(client) = crate::ontology_client::global() {
            let (types, objects, action_types, invocations, reviews) = tokio::join!(
                client.list_object_types(&session.bearer_token),
                client.list_objects(&session.bearer_token, None),
                client.list_action_types(&session.bearer_token),
                client.list_action_invocations(&session.bearer_token),
                client.list_action_reviews(&session.bearer_token),
            );
            let record_id = id.to_string();
            let object_types = types.unwrap_or_default();
            let journey_types = object_types
                .iter()
                .map(|item| JourneyObjectType { id: item.id, name: item.name.clone() })
                .collect::<Vec<_>>();
            let type_names = object_types
                .into_iter()
                .map(|item| (item.id, item.name))
                .collect::<std::collections::HashMap<_, _>>();
            let derived_objects = objects
                .unwrap_or_default()
                .into_iter()
                .filter(|object| {
                    object
                        .source_lineage
                        .as_array()
                        .map(|lineage| {
                            lineage.iter().any(|value| value.as_str() == Some(record_id.as_str()))
                        })
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            let modeled_objects = derived_objects
                .iter()
                .map(|object| {
                    let label = object
                        .properties
                        .get("name")
                        .or_else(|| object.properties.get("subject"))
                        .or_else(|| object.properties.get("title"))
                        .or_else(|| object.properties.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Untitled object")
                        .to_string();
                    ModeledJourneyObject {
                        id: object.id,
                        type_name: type_names
                            .get(&object.object_type_id)
                            .cloned()
                            .unwrap_or_else(|| "Modeled object".to_string()),
                        label,
                    }
                })
                .collect::<Vec<_>>();
            let object_labels = derived_objects
                .into_iter()
                .map(|object| {
                    let label = object
                        .properties
                        .get("name")
                        .or_else(|| object.properties.get("subject"))
                        .or_else(|| object.properties.get("title"))
                        .or_else(|| object.properties.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Untitled object")
                        .to_string();
                    (object.id, label)
                })
                .collect::<std::collections::HashMap<_, _>>();
            let action_names = action_types
                .unwrap_or_default()
                .into_iter()
                .map(|action| (action.id, action.name))
                .collect::<std::collections::HashMap<_, _>>();
            let action_reviews = reviews.unwrap_or_default();
            let now = chrono::Utc::now();
            let mut decisions = Vec::new();
            for invocation in invocations.unwrap_or_default() {
                let action_name = action_names
                    .get(&invocation.action_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Governed action".to_string());
                if let Some(targets) = invocation.target_object_ids.as_array() {
                    for target in targets {
                        let Some(target_id) =
                            target.as_str().and_then(|value| value.parse::<Uuid>().ok())
                        else {
                            continue;
                        };
                        let Some(target_label) = object_labels.get(&target_id).cloned() else {
                            continue;
                        };
                        decisions.push(GovernedDecision {
                            invocation_id: invocation.id,
                            action_name: action_name.clone(),
                            target_id,
                            target_label,
                            outcome: invocation.outcome.clone(),
                            executed_at: invocation.executed_at,
                            review_status: action_reviews
                                .iter()
                                .find(|review| review.invocation_id == invocation.id)
                                .map(|review| review.status.replace('_', " "))
                                .unwrap_or_else(|| "not reviewed".to_string()),
                            review_assignee: action_reviews
                                .iter()
                                .find(|review| review.invocation_id == invocation.id)
                                .and_then(|review| review.assignee.clone()),
                            review_stale: action_reviews
                                .iter()
                                .find(|review| review.invocation_id == invocation.id)
                                .map(|review| {
                                    !matches!(review.status.as_str(), "approved" | "declined")
                                        && review.due_at.is_some_and(|due_at| due_at <= now)
                                })
                                .unwrap_or(false),
                        });
                    }
                }
            }
            decisions.sort_by_key(|decision| std::cmp::Reverse(decision.executed_at));
            (modeled_objects, decisions, journey_types)
        } else {
            (vec![], vec![], vec![])
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
        let incident =
            incidents.iter().find(|item| item.event_ids.contains(&event.id)).map(|item| {
                JourneyIncident {
                    id: item.incident.id,
                    title: item.incident.title.clone(),
                    severity: item.incident.severity.to_string(),
                    status: item.incident.status.to_string(),
                }
            });
        event_links.push(EventLink { event, latency_from_ingest, executions, incident });
    }
    let event_count = event_links.len();
    let execution_count = event_links.iter().map(|link| link.executions.len()).sum();
    let incident_count = event_links.iter().filter(|link| link.incident.is_some()).count();
    let modeled_object_count = modeled_objects.len();
    let decision_count = governed_decisions.len();
    let timing = if let Some(record) = record.as_ref() {
        let max_latency_ms = event_links
            .iter()
            .flat_map(|link| {
                std::iter::once(link.event.occurred_at)
                    .chain(link.executions.iter().map(|execution| execution.execution.executed_at))
            })
            .map(|at| (at - record.ingested_at).num_milliseconds().max(0) as usize)
            .max()
            .unwrap_or(0)
            .max(1);
        let mut bars = Vec::new();
        for link in &event_links {
            let latency_ms =
                (link.event.occurred_at - record.ingested_at).num_milliseconds().max(0) as usize;
            bars.push(JourneyTimingBar {
                label: format!("Event · {}", link.event.event_type),
                detail: format!("{} · {}", link.event.group_key, link.event.status),
                href: format!("/events/{}", link.event.id),
                latency: format_latency(record.ingested_at, link.event.occurred_at),
                width_pct: if latency_ms == 0 {
                    8
                } else {
                    (latency_ms * 100 / max_latency_ms).max(8)
                },
                failure: false,
            });
            for execution in &link.executions {
                let latency_ms = (execution.execution.executed_at - record.ingested_at)
                    .num_milliseconds()
                    .max(0) as usize;
                bars.push(JourneyTimingBar {
                    label: format!("Action · {}", execution.execution.action_type),
                    detail: execution.execution.status.clone(),
                    href: format!("/events/{}", link.event.id),
                    latency: format_latency(record.ingested_at, execution.execution.executed_at),
                    width_pct: (latency_ms * 100 / max_latency_ms).max(8),
                    failure: execution.execution.status != "sent",
                });
            }
        }
        bars
    } else {
        vec![]
    };

    Html(
        RecordJourneyTemplate {
            show_nav: true,
            is_admin,
            source_record: record,
            event_links,
            timing,
            modeled_objects,
            object_types,
            can_write: session.role.at_least(common::Role::Operator),
            governed_decisions,
            error: None,
            event_count,
            execution_count,
            incident_count,
            modeled_object_count,
            decision_count,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
