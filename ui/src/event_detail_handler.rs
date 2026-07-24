#[path = "event_detail_handler_test.rs"]
#[cfg(test)]
mod event_detail_handler_test;

use crate::events_client::{EventDetail, StatusHistoryEntry};
use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
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

fn object_matches_event(
    object: &common::ontology::Object,
    entity_ref: &str,
    record_ids: &[Uuid],
) -> bool {
    object.properties.get("id").and_then(serde_json::Value::as_str) == Some(entity_ref)
        || object.id.to_string() == entity_ref
        || object
            .source_lineage
            .as_array()
            .map(|lineage| {
                record_ids.iter().any(|record_id| {
                    let record_id = record_id.to_string();
                    lineage.iter().any(|item| item.as_str() == Some(record_id.as_str()))
                })
            })
            .unwrap_or(false)
}

/// One timeline entry -- either the event firing itself or a subsequent action execution it
/// caused, pre-sorted chronologically so the template just renders top-to-bottom.
struct TimelineEntry {
    label: String,
    detail: String,
    at: chrono::DateTime<chrono::Utc>,
    latency_from_event: Option<String>,
    latency_pct: usize,
    is_failure: bool,
}

#[derive(Template)]
#[template(path = "event_detail.html")]
struct EventDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    event: Option<EventDetail>,
    status_class: String,
    payload_pretty: String,
    contributing_records: Vec<RecordSummary>,
    timeline: Vec<TimelineEntry>,
    linked_incidents: Vec<IncidentLink>,
    incident_options: Vec<IncidentLink>,
    modeled_context: Option<ModeledContext>,
    related_modeled_contexts: Vec<ModeledContext>,
    governed_actions: Vec<GovernedActionContext>,
    status_history: Vec<StatusHistoryEntry>,
    source_record_count: usize,
    execution_count: usize,
    linked_case_count: usize,
    modeled_object_count: usize,
    error: Option<String>,
    notice: String,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventDetailQuery {
    #[serde(default)]
    notice: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct EventStatusForm {
    pub status: String,
}

/// POST /events/:id/status — the Console's authenticated operator boundary for advancing a
/// signal's lifecycle. The dashboard/query services still enforce tenant scope from the bearer
/// token; this handler only owns redirect-and-notice UX.
pub async fn post_event_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<EventStatusForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "operator access required").into_response();
    }
    let status = form.status.trim().to_ascii_lowercase();
    if !matches!(status.as_str(), "new" | "triggered" | "actioned" | "dismissed") {
        return Redirect::to(&format!("/events/{id}?notice=invalid-status")).into_response();
    }
    let notice = match state
        .events_client
        .update_event_status(&session.bearer_token, id, &status, &session.username)
        .await
    {
        Ok(()) => "status-updated",
        Err(_) => "status-update-failed",
    };
    Redirect::to(&format!("/events/{id}?notice={notice}")).into_response()
}

struct IncidentLink {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
}

struct ModeledContext {
    id: Uuid,
    type_name: String,
    label: String,
    status: String,
}

struct GovernedActionContext {
    id: Uuid,
    name: String,
    target_type_name: String,
    eligible: bool,
    parameter_fields: Vec<EventActionParameterField>,
    preconditions: String,
    effect_definition: String,
    invocation_id: Option<Uuid>,
    invocation_outcome: Option<String>,
    review_status: String,
    review_assignee: Option<String>,
    review_stale: bool,
}

struct EventActionParameterField {
    name: String,
    field_type: String,
    required: bool,
    default_value: String,
}

fn action_parameter_fields(schema: &serde_json::Value) -> Vec<EventActionParameterField> {
    let mut fields = schema
        .as_object()
        .into_iter()
        .flat_map(|items| items.iter())
        .map(|(name, definition)| {
            let field_type = definition
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string")
                .to_string();
            let default_value = match field_type.as_str() {
                "boolean" => "false",
                "number" | "integer" => "0",
                "array" => "[]",
                "object" => "{}",
                _ => "",
            }
            .to_string();
            EventActionParameterField {
                name: name.clone(),
                field_type,
                required: definition
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                default_value,
            }
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.name.cmp(&right.name));
    fields
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

fn error_page(is_admin: bool, can_write: bool, message: String) -> Response {
    Html(
        EventDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            event: None,
            status_class: String::new(),
            payload_pretty: String::new(),
            contributing_records: vec![],
            timeline: vec![],
            linked_incidents: vec![],
            incident_options: vec![],
            modeled_context: None,
            related_modeled_contexts: vec![],
            governed_actions: vec![],
            status_history: vec![],
            source_record_count: 0,
            execution_count: 0,
            linked_case_count: 0,
            modeled_object_count: 0,
            error: Some(message),
            notice: String::new(),
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
    Query(query): Query<EventDetailQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let event = match state.events_client.get_event(&session.bearer_token, id).await {
        Ok(Some(event)) => event,
        Ok(None) => {
            return error_page(is_admin, can_write, "no event found with this id".to_string())
        }
        Err(e) => return error_page(is_admin, can_write, e.to_string()),
    };

    let status_history = state
        .events_client
        .list_status_history(&session.bearer_token, event.id)
        .await
        .unwrap_or_default();
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

    let all_incidents =
        state.incidents_client.list_incidents(session.tenant_id, None).await.unwrap_or_default();
    let linked_incidents: Vec<IncidentLink> = all_incidents
        .iter()
        .filter(|incident| incident.event_ids.contains(&event.id))
        .map(|incident| IncidentLink {
            id: incident.incident.id,
            title: incident.incident.title.clone(),
            severity: incident.incident.severity.to_string(),
            status: incident.incident.status.to_string(),
        })
        .collect();
    let incident_options: Vec<IncidentLink> = all_incidents
        .iter()
        .filter(|incident| !incident.event_ids.contains(&event.id))
        .map(|incident| IncidentLink {
            id: incident.incident.id,
            title: incident.incident.title.clone(),
            severity: incident.incident.severity.to_string(),
            status: incident.incident.status.to_string(),
        })
        .collect();

    let mut timeline = vec![TimelineEntry {
        label: "Event fired".to_string(),
        detail: event.event_type.clone(),
        at: event.occurred_at,
        latency_from_event: None,
        latency_pct: 8,
        is_failure: false,
    }];
    for execution in &executions {
        timeline.push(TimelineEntry {
            label: format!("Action: {}", execution.action_type),
            detail: execution.status.clone(),
            at: execution.executed_at,
            latency_from_event: Some(format_latency(event.occurred_at, execution.executed_at)),
            latency_pct: 8,
            is_failure: execution.status != "sent",
        });
    }

    // Governed ontology actions share the event's causal reference with the legacy executor,
    // so an operator sees the complete response chain in one timeline.
    if let Some(client) = crate::ontology_client::global() {
        let (invocations, action_types) = tokio::join!(
            client.list_action_invocations(&session.bearer_token),
            client.list_action_types(&session.bearer_token),
        );
        let action_names = action_types
            .unwrap_or_default()
            .into_iter()
            .map(|action| (action.id, action.name))
            .collect::<std::collections::HashMap<_, _>>();
        for invocation in invocations.unwrap_or_default() {
            let event_ref = invocation
                .triggering_event_ref
                .get("event_id")
                .or_else(|| invocation.triggering_event_ref.get("id"))
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok());
            if event_ref != Some(event.id) {
                continue;
            }
            timeline.push(TimelineEntry {
                label: format!(
                    "Governed action: {}",
                    action_names
                        .get(&invocation.action_type_id)
                        .map(String::as_str)
                        .unwrap_or("Unknown action")
                ),
                detail: invocation.outcome.clone(),
                at: invocation.executed_at,
                latency_from_event: Some(format_latency(event.occurred_at, invocation.executed_at)),
                latency_pct: 8,
                is_failure: !invocation.outcome.eq_ignore_ascii_case("completed"),
            });
        }
    }
    timeline.sort_by_key(|entry| entry.at);
    let max_latency_ms = timeline
        .iter()
        .map(|entry| (entry.at - event.occurred_at).num_milliseconds().max(0) as usize)
        .max()
        .unwrap_or(1);
    for entry in &mut timeline {
        let latency_ms = (entry.at - event.occurred_at).num_milliseconds().max(0) as usize;
        entry.latency_pct =
            if latency_ms == 0 { 8 } else { (latency_ms * 100 / max_latency_ms).max(8) };
    }
    let status_class = status_class(&event.status);
    let source_record_count = contributing_records.len();
    let execution_count = executions.len();
    let linked_case_count = linked_incidents.len();

    let (modeled_context, related_modeled_contexts, governed_actions) = if let Some(client) =
        crate::ontology_client::global()
    {
        let (types, objects, actions, invocations, reviews) = tokio::join!(
            client.list_object_types(&session.bearer_token),
            client.list_objects(&session.bearer_token, None),
            client.list_action_types(&session.bearer_token),
            client.list_action_invocations(&session.bearer_token),
            client.list_action_reviews(&session.bearer_token),
        );
        let type_names = types
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.id, item.name))
            .collect::<std::collections::HashMap<_, _>>();
        let matching_objects = objects
            .unwrap_or_default()
            .into_iter()
            .filter(|object| object_matches_event(object, &event.entity_ref, &event.record_ids))
            .collect::<Vec<_>>();
        let matching_object = matching_objects.first();
        let related_modeled_contexts = matching_objects
            .iter()
            .map(|object| ModeledContext {
                id: object.id,
                type_name: type_names
                    .get(&object.object_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Modeled object".to_string()),
                label: object
                    .properties
                    .get("name")
                    .or_else(|| object.properties.get("subject"))
                    .or_else(|| object.properties.get("title"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&event.entity_ref)
                    .to_string(),
                status: object
                    .properties
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("modeled")
                    .to_string(),
            })
            .collect::<Vec<_>>();
        let modeled_context = related_modeled_contexts.first().map(|context| ModeledContext {
            id: context.id,
            type_name: context.type_name.clone(),
            label: context.label.clone(),
            status: context.status.clone(),
        });
        let event_invocations = invocations
            .unwrap_or_default()
            .into_iter()
            .filter(|invocation| {
                let event_ref = invocation
                    .triggering_event_ref
                    .get("event_id")
                    .or_else(|| invocation.triggering_event_ref.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok());
                event_ref == Some(event.id)
            })
            .collect::<Vec<_>>();
        let action_reviews = reviews.unwrap_or_default();
        let now = chrono::Utc::now();
        let governed_actions = if let Some(object) = matching_object {
            actions
                .unwrap_or_default()
                .into_iter()
                .map(|action| {
                    let object_id = object.id.to_string();
                    let invocation = event_invocations
                        .iter()
                        .filter(|invocation| {
                            invocation.action_type_id == action.id
                                && invocation
                                    .target_object_ids
                                    .as_array()
                                    .map(|targets| {
                                        targets.iter().any(|target| {
                                            target.as_str() == Some(object_id.as_str())
                                        })
                                    })
                                    .unwrap_or(false)
                        })
                        .max_by_key(|invocation| invocation.executed_at);
                    let review = invocation.and_then(|invocation| {
                        action_reviews.iter().find(|review| review.invocation_id == invocation.id)
                    });
                    GovernedActionContext {
                        id: action.id,
                        name: action.name,
                        target_type_name: action
                            .target_object_type_id
                            .and_then(|type_id| type_names.get(&type_id).cloned())
                            .unwrap_or_else(|| "Any object type".to_string()),
                        eligible: action
                            .target_object_type_id
                            .map(|type_id| type_id == object.object_type_id)
                            .unwrap_or(true)
                            && action
                                .preconditions
                                .as_object()
                                .map(|preconditions| {
                                    preconditions.iter().all(|(key, expected)| {
                                        object.properties.get(key) == Some(expected)
                                    })
                                })
                                .unwrap_or(
                                    action.preconditions.is_null()
                                        || action.preconditions == serde_json::json!({}),
                                ),
                        parameter_fields: action_parameter_fields(&action.parameter_schema),
                        preconditions: serde_json::to_string_pretty(&action.preconditions)
                            .unwrap_or_default(),
                        effect_definition: serde_json::to_string_pretty(&action.effect_definition)
                            .unwrap_or_default(),
                        invocation_id: invocation.map(|invocation| invocation.id),
                        invocation_outcome: invocation.map(|invocation| invocation.outcome.clone()),
                        review_status: review
                            .map(|review| review.status.replace('_', " "))
                            .unwrap_or_else(|| "not reviewed".to_string()),
                        review_assignee: review.and_then(|review| review.assignee.clone()),
                        review_stale: review
                            .map(|review| {
                                !matches!(review.status.as_str(), "approved" | "declined")
                                    && review.due_at.is_some_and(|due_at| due_at <= now)
                            })
                            .unwrap_or(false),
                    }
                })
                .collect()
        } else {
            vec![]
        };
        (modeled_context, related_modeled_contexts, governed_actions)
    } else {
        (None, vec![], vec![])
    };
    let modeled_object_count = related_modeled_contexts.len();

    Html(
        EventDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            event: Some(event),
            status_class,
            payload_pretty,
            contributing_records,
            timeline,
            linked_incidents,
            incident_options,
            modeled_context,
            related_modeled_contexts,
            governed_actions,
            status_history,
            source_record_count,
            execution_count,
            linked_case_count,
            modeled_object_count,
            error: None,
            notice: query.notice,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
