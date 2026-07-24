use crate::execution_client::DeadLetterQueueSummary;
use crate::ontology_client::{self, ActionReviewRequest, InvokeActionRequest};
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use chrono::{DateTime, Utc};
use common::SavedSearchQuery;
use uuid::Uuid;

struct ActionDefinition {
    id: Uuid,
    name: String,
    target_type_name: String,
    parameter_schema: String,
    parameter_example: String,
    parameter_fields: Vec<ActionParameterField>,
    preconditions: String,
    effect_definition: String,
    targets: Vec<ActionTargetOption>,
    eligible_target_count: usize,
}

struct ActionParameterField {
    name: String,
    field_type: String,
    required: bool,
    default_value: String,
}

struct ActionTarget {
    id: Uuid,
    label: String,
    type_name: String,
}

struct ActionSourceRecord {
    id: Uuid,
    preview: String,
    connector_id: String,
    ingested_at: chrono::DateTime<chrono::Utc>,
}

struct ActionTargetOption {
    id: Uuid,
    label: String,
    type_name: String,
    eligible: bool,
    reason: String,
}

struct InvocationRow {
    id: Uuid,
    action_type_id: Uuid,
    action_name: String,
    outcome: String,
    review_status: String,
    review_assignee: String,
    review_stale: bool,
    executed_at: chrono::DateTime<chrono::Utc>,
    parameters: String,
    audit_context: String,
    event_id: Option<Uuid>,
    incident_id: Option<Uuid>,
    target_object_ids: String,
    targets: Vec<ActionTarget>,
}

struct ActionInvocationDetail {
    id: Uuid,
    action_type_id: Uuid,
    action_name: String,
    target_type_name: String,
    parameter_schema: String,
    preconditions: String,
    effect_definition: String,
    outcome: String,
    executed_at: chrono::DateTime<chrono::Utc>,
    parameters: String,
    audit_context: String,
    target_object_ids: String,
    targets: Vec<ActionTarget>,
    source_records: Vec<ActionSourceRecord>,
    event_id: Option<Uuid>,
    incident_id: Option<Uuid>,
    contract_snapshotted: bool,
}

#[derive(Template)]
#[template(path = "action_detail.html")]
struct ActionDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    invocation: Option<ActionInvocationDetail>,
    review: Option<common::ontology::ActionReview>,
    review_due_at: String,
    review_stale: bool,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "actions.html")]
struct ActionsTemplate {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    action_types: Vec<ActionDefinition>,
    objects: Vec<ActionTarget>,
    invocations: Vec<InvocationRow>,
    completed_count: usize,
    failed_count: usize,
    visible_count: usize,
    matching_count: usize,
    page: i64,
    has_prev: bool,
    has_next: bool,
    dead_letter_count: u32,
    dead_letter_queues: Vec<DeadLetterQueueSummary>,
    replayed: String,
    notice: String,
    q: String,
    outcome: String,
    action_filter: String,
    from: String,
    to: String,
    review: String,
    error: Option<String>,
    saved_views: Vec<SavedActionView>,
    retried_count: usize,
    retry_failed_count: usize,
    review_saved: usize,
    review_failed: usize,
    outcome_metrics: Vec<ActionPostureMetric>,
    review_metrics: Vec<ActionPostureMetric>,
    review_matrix: Vec<ActionReviewMatrixRow>,
    trend: Vec<ActionTrendBar>,
}

struct ActionPostureMetric {
    label: String,
    count: usize,
    percent: i32,
    href: String,
    tone: String,
}

struct ActionReviewMatrixRow {
    label: String,
    tone: String,
    total: usize,
    unreviewed: usize,
    assigned: usize,
    overdue: usize,
}

struct ActionTrendBar {
    date: String,
    completed: usize,
    failed: usize,
    height_pct: u32,
}

fn action_trend(invocations: &[InvocationRow]) -> Vec<ActionTrendBar> {
    let mut buckets = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for invocation in invocations {
        let entry =
            buckets.entry(invocation.executed_at.format("%Y-%m-%d").to_string()).or_default();
        if invocation.outcome.eq_ignore_ascii_case("completed") {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let max = buckets.values().map(|(completed, failed)| completed + failed).max().unwrap_or(1);
    buckets
        .into_iter()
        .map(|(date, (completed, failed))| ActionTrendBar {
            date,
            completed,
            failed,
            height_pct: (((completed + failed) * 100) / max).max(8) as u32,
        })
        .collect()
}

fn action_review_matrix(invocations: &[InvocationRow]) -> Vec<ActionReviewMatrixRow> {
    [("Completed", "good", true), ("Needs review", "risk", false)]
        .into_iter()
        .map(|(label, tone, completed)| {
            let items = invocations
                .iter()
                .filter(|item| item.outcome.eq_ignore_ascii_case("completed") == completed)
                .collect::<Vec<_>>();
            ActionReviewMatrixRow {
                label: label.to_string(),
                tone: tone.to_string(),
                total: items.len(),
                unreviewed: items
                    .iter()
                    .filter(|item| item.review_status == "not reviewed")
                    .count(),
                assigned: items.iter().filter(|item| !item.review_assignee.is_empty()).count(),
                overdue: items.iter().filter(|item| item.review_stale).count(),
            }
        })
        .collect()
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default, Clone)]
struct SavedActionFilter {
    #[serde(default)]
    q: String,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    review: String,
}

#[derive(Clone)]
struct SavedActionView {
    id: Uuid,
    name: String,
    load_url: String,
}

fn to_saved_action_view(query: SavedSearchQuery) -> SavedActionView {
    let filter: SavedActionFilter = serde_json::from_value(query.filter).unwrap_or_default();
    let load_url = format!("/actions?{}", serde_urlencoded::to_string(&filter).unwrap_or_default());
    SavedActionView { id: query.id, name: query.name, load_url }
}

const ACTION_PAGE_SIZE: usize = 25;

#[derive(Debug, serde::Deserialize, Default)]
pub struct ActionsQuery {
    #[serde(default)]
    pub page: i64,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub review: String,
    #[serde(default)]
    pub replayed: String,
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub retried_count: usize,
    #[serde(default)]
    pub retry_failed_count: usize,
    #[serde(default)]
    pub review_saved: usize,
    #[serde(default)]
    pub review_failed: usize,
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn action_matches_filter(action_name: &str, filter: &str) -> bool {
    filter.trim().is_empty() || action_name.eq_ignore_ascii_case(filter.trim())
}

fn review_is_stale(due_at: Option<DateTime<Utc>>, status: &str, now: DateTime<Utc>) -> bool {
    !matches!(status, "approved" | "declined") && due_at.is_some_and(|due_at| due_at <= now)
}

fn review_matches_filter(status: &str, assignee: &str, stale: bool, filter: &str) -> bool {
    match filter.trim().to_ascii_lowercase().as_str() {
        "unreviewed" => status == "not reviewed",
        "open" => status == "open",
        "in_progress" => status == "in progress",
        "approved" => status == "approved",
        "declined" => status == "declined",
        "handed_off" => status == "handed off",
        "assigned" => !assignee.is_empty(),
        "stale" => stale,
        _ => true,
    }
}

fn action_matches_search(
    id: Uuid,
    action_name: &str,
    target_object_ids: &str,
    parameters: &str,
    audit_context: &str,
    search: &str,
) -> bool {
    let search = search.trim().to_ascii_lowercase();
    if search.is_empty() {
        return true;
    }
    format!("{id} {action_name} {target_object_ids} {parameters} {audit_context}")
        .to_ascii_lowercase()
        .contains(&search)
}

fn parse_date_range(from: &str, to: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let parse = |value: &str, end_of_day: bool| {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .ok()
            .and_then(|date| {
                date.and_hms_opt(
                    if end_of_day { 23 } else { 0 },
                    if end_of_day { 59 } else { 0 },
                    if end_of_day { 59 } else { 0 },
                )
            })
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    };
    (parse(from, false), parse(to, true))
}

fn bulk_retry_redirect(
    fields: &[(String, String)],
    notice: &str,
    retried_count: usize,
    retry_failed_count: usize,
) -> axum::response::Redirect {
    let value = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let query = serde_urlencoded::to_string([
        ("q", value("q")),
        ("outcome", value("outcome")),
        ("action", value("action")),
        ("from", value("from")),
        ("to", value("to")),
        ("review", value("review")),
        ("notice", notice.to_string()),
        ("retried_count", retried_count.to_string()),
        ("retry_failed_count", retry_failed_count.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    axum::response::Redirect::to(&format!("/actions?{query}"))
}

fn action_view_redirect(form: &SaveActionViewForm, notice: &str) -> axum::response::Redirect {
    let query = serde_urlencoded::to_string([
        ("q", form.q.clone()),
        ("outcome", form.outcome.clone()),
        ("action", form.action.clone()),
        ("from", form.from.clone()),
        ("to", form.to.clone()),
        ("review", form.review.clone()),
        ("notice", notice.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    axum::response::Redirect::to(&format!("/actions?{query}"))
}

/// POST /actions/bulk-retry — raw form parsing preserves repeated selected invocation IDs.
pub async fn post_bulk_retry_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "operator access required").into_response();
    }
    let fields = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body).unwrap_or_default();
    let ids = fields
        .iter()
        .filter(|(name, _)| name == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return bulk_retry_redirect(&fields, "bulk-retry-empty", 0, 0).into_response();
    }
    let Some(client) = ontology_client::global() else {
        return bulk_retry_redirect(&fields, "bulk-retry-failed", 0, ids.len()).into_response();
    };
    let invocations = match client.list_action_invocations(&session.bearer_token).await {
        Ok(invocations) => invocations,
        Err(_) => {
            return bulk_retry_redirect(&fields, "bulk-retry-failed", 0, ids.len()).into_response()
        }
    };
    let mut retried_count = 0usize;
    let mut retry_failed_count = 0usize;
    for id in ids {
        let Some(invocation) = invocations.iter().find(|invocation| {
            invocation.id == id && !invocation.outcome.eq_ignore_ascii_case("completed")
        }) else {
            retry_failed_count += 1;
            continue;
        };
        let target_object_ids = invocation
            .target_object_ids
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter_map(|value| value.parse::<Uuid>().ok())
            .collect::<Vec<_>>();
        if target_object_ids.is_empty() {
            retry_failed_count += 1;
            continue;
        }
        let request = InvokeActionRequest {
            action_type_id: invocation.action_type_id,
            target_object_ids,
            parameters: invocation.parameters.clone(),
            triggering_event_ref: Some(invocation.triggering_event_ref.clone()),
        };
        match client.invoke_action(&session.bearer_token, &request).await {
            Ok(_) => retried_count += 1,
            Err(_) => retry_failed_count += 1,
        }
    }
    bulk_retry_redirect(&fields, "bulk-retry-complete", retried_count, retry_failed_count)
        .into_response()
}

fn bulk_review_redirect(
    fields: &[(String, String)],
    notice: &str,
    status: &str,
    saved: usize,
    failed: usize,
) -> axum::response::Redirect {
    let value = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let query = serde_urlencoded::to_string([
        ("q", value("q")),
        ("outcome", value("outcome")),
        ("action", value("action")),
        ("from", value("from")),
        ("to", value("to")),
        ("review", status.to_string()),
        ("notice", notice.to_string()),
        ("review_saved", saved.to_string()),
        ("review_failed", failed.to_string()),
    ])
    .unwrap_or_else(|_| format!("notice={notice}"));
    axum::response::Redirect::to(&format!("/actions?{query}"))
}

/// POST /actions/bulk-review — apply a governed human-review transition to selected invocations.
pub async fn post_bulk_action_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (axum::http::StatusCode::FORBIDDEN, "operator access required").into_response();
    }
    let fields = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body).unwrap_or_default();
    let status = fields
        .iter()
        .find(|(name, _)| name == "status")
        .map(|(_, value)| value.as_str())
        .unwrap_or("open");
    if !matches!(status, "open" | "in_progress" | "approved" | "declined" | "handed_off") {
        return bulk_review_redirect(&fields, "bulk-review-failed", status, 0, 0).into_response();
    }
    if matches!(status, "approved" | "declined") && !session.role.at_least(common::Role::Admin) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "admin access required for final review transitions",
        )
            .into_response();
    }
    let ids = fields
        .iter()
        .filter(|(name, _)| name == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return bulk_review_redirect(&fields, "bulk-review-empty", status, 0, 0).into_response();
    }
    let Some(client) = ontology_client::global() else {
        return bulk_review_redirect(&fields, "bulk-review-failed", status, 0, ids.len())
            .into_response();
    };
    let known_ids = match client.list_action_invocations(&session.bearer_token).await {
        Ok(invocations) => invocations
            .into_iter()
            .map(|invocation| invocation.id)
            .collect::<std::collections::HashSet<_>>(),
        Err(_) => {
            return bulk_review_redirect(&fields, "bulk-review-failed", status, 0, ids.len())
                .into_response()
        }
    };
    let assignee = fields
        .iter()
        .find(|(name, _)| name == "assignee")
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let note = fields
        .iter()
        .find(|(name, _)| name == "note")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let due_at = fields
        .iter()
        .find(|(name, _)| name == "due_at")
        .and_then(|(_, value)| parse_review_due_at(value));
    let mut saved = 0usize;
    let mut failed = 0usize;
    for id in ids {
        if !known_ids.contains(&id) {
            failed += 1;
            continue;
        }
        let request = ActionReviewRequest {
            invocation_id: id,
            status: status.to_string(),
            assignee: assignee.clone(),
            note: note.clone(),
            due_at,
        };
        match client.upsert_action_review(&session.bearer_token, &request).await {
            Ok(_) => saved += 1,
            Err(_) => failed += 1,
        }
    }
    bulk_review_redirect(&fields, "bulk-review-complete", status, saved, failed).into_response()
}

fn action_detail_page(
    is_admin: bool,
    can_manage: bool,
    invocation: Option<ActionInvocationDetail>,
    review: Option<common::ontology::ActionReview>,
    error: Option<String>,
) -> Response {
    let review_due_at = review
        .as_ref()
        .and_then(|review| review.due_at)
        .map(|due_at| due_at.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default();
    Html(
        ActionDetailTemplate {
            show_nav: true,
            is_admin,
            can_manage,
            review_stale: review
                .as_ref()
                .is_some_and(|review| review_is_stale(review.due_at, &review.status, Utc::now())),
            invocation,
            review,
            review_due_at,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

/// GET /actions/:id — a focused evidence page for one immutable governed decision.
pub async fn get_action_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_manage = session.role.at_least(common::Role::Operator);
    let Some(client) = ontology_client::global() else {
        return action_detail_page(
            is_admin,
            can_manage,
            None,
            None,
            Some("Ontology client is not configured".into()),
        );
    };
    let token = session.bearer_token.as_str();
    let (action_types, invocations, objects, object_types, reviews) = tokio::join!(
        client.list_action_types(token),
        client.list_action_invocations(token),
        client.list_objects(token, None),
        client.list_object_types(token),
        client.list_action_reviews(token),
    );
    let invocations = match invocations {
        Ok(value) => value,
        Err(error) => {
            return action_detail_page(is_admin, can_manage, None, None, Some(error.to_string()))
        }
    };
    let Some(invocation) = invocations.into_iter().find(|item| item.id == id) else {
        return action_detail_page(
            is_admin,
            can_manage,
            None,
            None,
            Some("governed invocation not found".into()),
        );
    };
    let action_types = action_types.unwrap_or_default();
    let action = action_types.iter().find(|item| item.id == invocation.action_type_id);
    let snapshot = invocation.contract_snapshot.as_ref().and_then(serde_json::Value::as_object);
    let contract_value = |field: &str| {
        snapshot.and_then(|value| value.get(field)).or_else(|| {
            action.and_then(|item| match field {
                "parameter_schema" => Some(&item.parameter_schema),
                "preconditions" => Some(&item.preconditions),
                "effect_definition" => Some(&item.effect_definition),
                _ => None,
            })
        })
    };
    let object_type_names = object_types
        .unwrap_or_default()
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let objects = objects.unwrap_or_default();
    let target_ids = invocation
        .target_object_ids
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|value| value.parse::<Uuid>().ok())
        .collect::<Vec<_>>();
    let targets = target_ids
        .iter()
        .filter_map(|target_id| {
            let object = objects.iter().find(|object| object.id == *target_id)?;
            Some(ActionTarget {
                id: object.id,
                label: object_label(&object.properties, &object.id.to_string()),
                type_name: object_type_names
                    .get(&object.object_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Modeled object".into()),
            })
        })
        .collect::<Vec<_>>();
    let event_id = invocation
        .triggering_event_ref
        .get("event_id")
        .or_else(|| invocation.triggering_event_ref.get("id"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok());
    let incident_id = invocation
        .triggering_event_ref
        .get("incident_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok());
    let mut source_records = Vec::new();
    if let Some(event_id) = event_id {
        if let Ok(Some(event)) = state.events_client.get_event(token, event_id).await {
            for record_id in event.record_ids {
                if let Ok(Some(record)) =
                    state.stats_client.get_record(session.tenant_id, record_id).await
                {
                    source_records.push(ActionSourceRecord {
                        id: record.id,
                        preview: record.preview(),
                        connector_id: record.connector_id,
                        ingested_at: record.ingested_at,
                    });
                }
            }
        }
    }
    let detail = ActionInvocationDetail {
        id: invocation.id,
        action_type_id: invocation.action_type_id,
        action_name: action
            .map(|item| item.name.clone())
            .unwrap_or_else(|| "Governed action".into()),
        target_type_name: action
            .and_then(|item| item.target_object_type_id)
            .and_then(|type_id| object_type_names.get(&type_id).cloned())
            .unwrap_or_else(|| "Any modeled object".into()),
        parameter_schema: contract_value("parameter_schema")
            .and_then(|value| serde_json::to_string_pretty(value).ok())
            .unwrap_or_default(),
        preconditions: contract_value("preconditions")
            .and_then(|value| serde_json::to_string_pretty(value).ok())
            .unwrap_or_default(),
        effect_definition: contract_value("effect_definition")
            .and_then(|value| serde_json::to_string_pretty(value).ok())
            .unwrap_or_default(),
        outcome: invocation.outcome,
        executed_at: invocation.executed_at,
        parameters: serde_json::to_string_pretty(&invocation.parameters).unwrap_or_default(),
        audit_context: serde_json::to_string_pretty(&invocation.triggering_event_ref)
            .unwrap_or_default(),
        target_object_ids: serde_json::to_string(&invocation.target_object_ids).unwrap_or_default(),
        targets,
        source_records,
        event_id,
        incident_id,
        contract_snapshotted: invocation.contract_snapshot.is_some(),
    };
    let review =
        reviews.ok().and_then(|items| items.into_iter().find(|review| review.invocation_id == id));
    action_detail_page(is_admin, can_manage, Some(detail), review, None)
}

#[derive(Debug, serde::Deserialize)]
pub struct ActionReviewForm {
    status: String,
    assignee: String,
    note: String,
    due_at: String,
}

fn parse_review_due_at(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%dT%H:%M")
        .ok()
        .map(|value| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(value, chrono::Utc))
}

/// POST /actions/:id/review — records the human review/handoff without mutating the immutable
/// action invocation itself.
pub async fn post_action_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionReviewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let Some(client) = ontology_client::global() else {
        return axum::response::Redirect::to(&format!("/actions/{id}?notice=review-failed"))
            .into_response();
    };
    let request = ActionReviewRequest {
        invocation_id: id,
        status: form.status,
        assignee: Some(form.assignee),
        note: form.note,
        due_at: parse_review_due_at(&form.due_at),
    };
    let notice = match client.upsert_action_review(&session.bearer_token, &request).await {
        Ok(_) => "review-saved",
        Err(_) => "review-failed",
    };
    axum::response::Redirect::to(&format!("/actions/{id}?notice={notice}")).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveActionViewForm {
    name: String,
    q: String,
    outcome: String,
    action: String,
    from: String,
    to: String,
    review: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ReplayDeadLetterForm {
    service: String,
}

pub async fn post_replay_dead_letter(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<ReplayDeadLetterForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    match state.execution_client.replay_dead_letter_queue(&form.service).await {
        Ok(true) => axum::response::Redirect::to("/actions?replayed=1").into_response(),
        Ok(false) => axum::response::Redirect::to("/actions?replayed=0").into_response(),
        Err(_) => axum::response::Redirect::to("/actions?replayed=error").into_response(),
    }
}

pub async fn post_save_action_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveActionViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let filter = serde_json::json!({ "view_kind": "actions", "q": form.q.clone(), "outcome": form.outcome.clone(), "action": form.action.clone(), "from": form.from.clone(), "to": form.to.clone(), "review": form.review.clone() });
    match state.saved_search_queries_client.create(session.tenant_id, &form.name, filter).await {
        Ok(_) => action_view_redirect(&form, "view_saved").into_response(),
        Err(_) => action_view_redirect(&form, "view_failed").into_response(),
    }
}

pub async fn post_delete_action_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(()) => axum::response::Redirect::to("/actions").into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

fn object_label(properties: &serde_json::Value, fallback: &str) -> String {
    properties
        .get("name")
        .or_else(|| properties.get("subject"))
        .or_else(|| properties.get("title"))
        .or_else(|| properties.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn parameter_example(schema: &serde_json::Value) -> String {
    let mut example = serde_json::Map::new();
    if let Some(fields) = schema.as_object() {
        for (name, definition) in fields {
            let value = match definition.get("type").and_then(serde_json::Value::as_str) {
                Some("boolean") => serde_json::Value::Bool(false),
                Some("number") | Some("integer") => serde_json::Value::Number(0.into()),
                Some("array") => serde_json::Value::Array(vec![]),
                Some("object") => serde_json::json!({}),
                _ => serde_json::Value::String(String::new()),
            };
            example.insert(name.clone(), value);
        }
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(example))
        .unwrap_or_else(|_| "{}".into())
}

fn parameter_fields(schema: &serde_json::Value) -> Vec<ActionParameterField> {
    schema
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.iter())
        .map(|(name, definition)| {
            let field_type = definition
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string")
                .to_string();
            let default_value = match field_type.as_str() {
                "boolean" => "false".to_string(),
                "number" | "integer" => "0".to_string(),
                "array" => "[]".to_string(),
                "object" => "{}".to_string(),
                _ => String::new(),
            };
            ActionParameterField {
                name: name.clone(),
                field_type,
                required: definition
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                default_value,
            }
        })
        .collect()
}

fn satisfies_preconditions(
    properties: &serde_json::Value,
    preconditions: &serde_json::Value,
) -> bool {
    let Some(preconditions) = preconditions.as_object() else {
        return preconditions.is_null() || preconditions == &serde_json::json!({});
    };
    let Some(properties) = properties.as_object() else {
        return false;
    };
    preconditions.iter().all(|(key, expected)| properties.get(key) == Some(expected))
}

fn eligibility_reason(properties: &serde_json::Value, preconditions: &serde_json::Value) -> String {
    if satisfies_preconditions(properties, preconditions) {
        return "Eligible".to_string();
    }
    let Some(preconditions) = preconditions.as_object() else {
        return "Preconditions not satisfied".to_string();
    };
    for (key, expected) in preconditions {
        if properties.get(key) != Some(expected) {
            let expected = serde_json::to_string(expected).unwrap_or_else(|_| expected.to_string());
            let actual = properties
                .get(key)
                .map(|value| serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
                .unwrap_or_else(|| "missing".to_string());
            return format!("Requires {key} = {expected}; current {actual}");
        }
    }
    "Preconditions not satisfied".to_string()
}

pub async fn get_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActionsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_manage = session.role.at_least(common::Role::Operator);
    let saved_views = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|query| {
            query.filter.get("view_kind").and_then(serde_json::Value::as_str) == Some("actions")
        })
        .map(to_saved_action_view)
        .collect::<Vec<_>>();
    let Some(client) = ontology_client::global() else {
        return Html(
            ActionsTemplate {
                show_nav: true,
                is_admin,
                can_manage,
                action_types: vec![],
                objects: vec![],
                invocations: vec![],
                completed_count: 0,
                failed_count: 0,
                visible_count: 0,
                matching_count: 0,
                page: query.page.max(0),
                has_prev: false,
                has_next: false,
                dead_letter_count: 0,
                dead_letter_queues: vec![],
                replayed: query.replayed,
                notice: query.notice,
                q: query.q,
                outcome: query.outcome,
                action_filter: query.action,
                from: query.from,
                to: query.to,
                review: query.review,
                error: Some("Ontology client is not configured".into()),
                saved_views,
                retried_count: query.retried_count,
                retry_failed_count: query.retry_failed_count,
                review_saved: query.review_saved,
                review_failed: query.review_failed,
                outcome_metrics: vec![],
                review_metrics: vec![],
                review_matrix: vec![],
                trend: vec![],
            }
            .render()
            .unwrap(),
        )
        .into_response();
    };
    let token = session.bearer_token.as_str();
    let (types, invocations, raw_objects, raw_object_types, reviews, dead_letter_queues) = tokio::join!(
        client.list_action_types(token),
        client.list_action_invocations(token),
        client.list_objects(token, None),
        client.list_object_types(token),
        client.list_action_reviews(token),
        state.execution_client.dead_letter_queues(),
    );
    let error = [
        &types.as_ref().err(),
        &invocations.as_ref().err(),
        &raw_objects.as_ref().err(),
        &raw_object_types.as_ref().err(),
        &reviews.as_ref().err(),
    ]
    .into_iter()
    .flatten()
    .next()
    .map(ToString::to_string);
    let action_types_raw = types.unwrap_or_default();
    let review_by_invocation = reviews
        .unwrap_or_default()
        .into_iter()
        .map(|review| (review.invocation_id, review))
        .collect::<std::collections::HashMap<_, _>>();
    let type_names = raw_object_types
        .unwrap_or_default()
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let raw_objects = raw_objects.unwrap_or_default();
    let object_type_ids = raw_objects
        .iter()
        .map(|object| (object.id, object.object_type_id))
        .collect::<std::collections::HashMap<_, _>>();
    let object_properties = raw_objects
        .iter()
        .map(|object| (object.id, object.properties.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut objects = raw_objects
        .into_iter()
        .map(|object| ActionTarget {
            id: object.id,
            label: object_label(&object.properties, "Untitled object"),
            type_name: type_names
                .get(&object.object_type_id)
                .cloned()
                .unwrap_or_else(|| "Entity".into()),
        })
        .collect::<Vec<_>>();
    // Put an entity satisfying at least one governed action's preconditions first. The
    // selector still exposes every modeled object, but the default target is now the one the
    // contract can actually operate on (for example, the investigating ticket rather than the
    // unrelated customer in the seeded workspace).
    objects.sort_by_key(|object| {
        let eligible = action_types_raw.iter().any(|action| {
            action
                .target_object_type_id
                .map(|type_id| object_type_ids.get(&object.id) == Some(&type_id))
                .unwrap_or(true)
                && object_properties
                    .get(&object.id)
                    .map(|properties| satisfies_preconditions(properties, &action.preconditions))
                    .unwrap_or(false)
        });
        !eligible
    });
    let action_types = action_types_raw
        .iter()
        .map(|action| {
            let mut targets = objects
                .iter()
                .map(|object| ActionTargetOption {
                    id: object.id,
                    label: object.label.clone(),
                    type_name: object.type_name.clone(),
                    eligible: action
                        .target_object_type_id
                        .map(|type_id| object_type_ids.get(&object.id) == Some(&type_id))
                        .unwrap_or(true)
                        && object_properties
                            .get(&object.id)
                            .map(|properties| {
                                satisfies_preconditions(properties, &action.preconditions)
                            })
                            .unwrap_or(false),
                    reason: if let Some(expected_type_id) = action
                        .target_object_type_id
                        .filter(|type_id| object_type_ids.get(&object.id) != Some(type_id))
                    {
                        format!(
                            "Requires target type {}",
                            type_names
                                .get(&expected_type_id)
                                .cloned()
                                .unwrap_or_else(|| expected_type_id.to_string())
                        )
                    } else {
                        object_properties
                            .get(&object.id)
                            .map(|properties| eligibility_reason(properties, &action.preconditions))
                            .unwrap_or_else(|| "Object properties unavailable".to_string())
                    },
                })
                .collect::<Vec<_>>();
            targets.sort_by_key(|target| !target.eligible);
            let eligible_target_count = targets.iter().filter(|target| target.eligible).count();
            ActionDefinition {
                id: action.id,
                name: action.name.clone(),
                target_type_name: action
                    .target_object_type_id
                    .and_then(|id| type_names.get(&id).cloned())
                    .unwrap_or_else(|| "Any object type".to_string()),
                parameter_schema: serde_json::to_string_pretty(&action.parameter_schema)
                    .unwrap_or_default(),
                parameter_example: parameter_example(&action.parameter_schema),
                parameter_fields: parameter_fields(&action.parameter_schema),
                preconditions: serde_json::to_string_pretty(&action.preconditions)
                    .unwrap_or_default(),
                effect_definition: serde_json::to_string_pretty(&action.effect_definition)
                    .unwrap_or_default(),
                targets,
                eligible_target_count,
            }
        })
        .collect::<Vec<_>>();
    let action_names = action_types_raw
        .into_iter()
        .map(|action| (action.id, action.name))
        .collect::<std::collections::HashMap<_, _>>();
    let object_map = objects
        .iter()
        .map(|object| (object.id, (object.label.clone(), object.type_name.clone())))
        .collect::<std::collections::HashMap<_, _>>();
    let mut completed_count = 0;
    let mut failed_count = 0;
    let mut invocations = invocations
        .unwrap_or_default()
        .into_iter()
        .map(|invocation| {
            if invocation.outcome.eq_ignore_ascii_case("completed") {
                completed_count += 1;
            } else {
                failed_count += 1;
            }
            let target_ids = invocation
                .target_object_ids
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str())
                .filter_map(|value| value.parse::<Uuid>().ok());
            let targets = target_ids
                .filter_map(|id| {
                    object_map.get(&id).map(|(label, type_name)| ActionTarget {
                        id,
                        label: label.clone(),
                        type_name: type_name.clone(),
                    })
                })
                .collect();
            let event_id = invocation
                .triggering_event_ref
                .get("event_id")
                .or_else(|| invocation.triggering_event_ref.get("id"))
                .and_then(serde_json::Value::as_str)
                .and_then(|value| value.parse::<Uuid>().ok());
            let incident_id = invocation
                .triggering_event_ref
                .get("incident_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| value.parse::<Uuid>().ok());
            let target_object_ids = invocation
                .target_object_ids
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            InvocationRow {
                id: invocation.id,
                action_type_id: invocation.action_type_id,
                action_name: action_names
                    .get(&invocation.action_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown action".into()),
                outcome: invocation.outcome,
                review_status: review_by_invocation
                    .get(&invocation.id)
                    .map(|review| review.status.replace('_', " "))
                    .unwrap_or_else(|| "not reviewed".to_string()),
                review_assignee: review_by_invocation
                    .get(&invocation.id)
                    .and_then(|review| review.assignee.clone())
                    .unwrap_or_default(),
                review_stale: review_by_invocation
                    .get(&invocation.id)
                    .map(|review| review_is_stale(review.due_at, &review.status, Utc::now()))
                    .unwrap_or(false),
                executed_at: invocation.executed_at,
                parameters: serde_json::to_string_pretty(&invocation.parameters)
                    .unwrap_or_default(),
                audit_context: serde_json::to_string_pretty(&invocation.triggering_event_ref)
                    .unwrap_or_default(),
                event_id,
                incident_id,
                target_object_ids,
                targets,
            }
        })
        .collect::<Vec<_>>();
    let search = query.q.trim().to_ascii_lowercase();
    let (from, to) = parse_date_range(&query.from, &query.to);
    invocations.retain(|invocation| {
        let matches_outcome = match query.outcome.as_str() {
            "completed" => invocation.outcome.eq_ignore_ascii_case("completed"),
            "review" => !invocation.outcome.eq_ignore_ascii_case("completed"),
            _ => true,
        };
        let action_matches = action_matches_filter(&invocation.action_name, &query.action);
        let review_matches = review_matches_filter(
            &invocation.review_status,
            &invocation.review_assignee,
            invocation.review_stale,
            &query.review,
        );
        let date_matches = from.map(|value| invocation.executed_at >= value).unwrap_or(true)
            && to.map(|value| invocation.executed_at <= value).unwrap_or(true);
        matches_outcome
            && action_matches
            && review_matches
            && date_matches
            && (action_matches_search(
                invocation.id,
                &invocation.action_name,
                &invocation.target_object_ids,
                &invocation.parameters,
                &invocation.audit_context,
                &search,
            ) || invocation.targets.iter().any(|target| {
                format!("{} {}", target.label, target.type_name)
                    .to_ascii_lowercase()
                    .contains(&search)
            }))
    });
    let matching_count = invocations.len();
    let metric =
        |label: &str, count: usize, total: usize, href: &str, tone: &str| ActionPostureMetric {
            label: label.to_string(),
            count,
            percent: if total == 0 { 0 } else { (count * 100 / total) as i32 },
            href: href.to_string(),
            tone: tone.to_string(),
        };
    let outcome_metrics = vec![
        metric(
            "Completed",
            invocations
                .iter()
                .filter(|item| item.outcome.eq_ignore_ascii_case("completed"))
                .count(),
            matching_count,
            "/actions?outcome=completed",
            "good",
        ),
        metric(
            "Needs review",
            invocations
                .iter()
                .filter(|item| !item.outcome.eq_ignore_ascii_case("completed"))
                .count(),
            matching_count,
            "/actions?outcome=review",
            "risk",
        ),
    ];
    let review_metrics = vec![
        metric(
            "Overdue",
            invocations.iter().filter(|item| item.review_stale).count(),
            matching_count,
            "/actions?review=stale",
            "risk",
        ),
        metric(
            "Unreviewed",
            invocations.iter().filter(|item| item.review_status == "not reviewed").count(),
            matching_count,
            "/actions?review=unreviewed",
            "warning",
        ),
        metric(
            "Assigned",
            invocations.iter().filter(|item| !item.review_assignee.is_empty()).count(),
            matching_count,
            "/actions?review=assigned",
            "neutral",
        ),
    ];
    let review_matrix = action_review_matrix(&invocations);
    let trend = action_trend(&invocations);
    let page = query.page.max(0);
    let start = (page as usize).saturating_mul(ACTION_PAGE_SIZE);
    let has_prev = page > 0;
    let has_next = start.saturating_add(ACTION_PAGE_SIZE) < matching_count;
    invocations = invocations.into_iter().skip(start).take(ACTION_PAGE_SIZE).collect();
    let visible_count = invocations.len();
    let dead_letter_queues = dead_letter_queues.unwrap_or_default();
    let dead_letter_count = dead_letter_queues.iter().filter_map(|queue| queue.count).sum();
    Html(
        ActionsTemplate {
            show_nav: true,
            is_admin,
            can_manage,
            action_types,
            objects,
            invocations,
            completed_count,
            failed_count,
            visible_count,
            matching_count,
            page,
            has_prev,
            has_next,
            dead_letter_count,
            dead_letter_queues,
            replayed: query.replayed,
            notice: query.notice,
            q: query.q,
            outcome: query.outcome,
            action_filter: query.action,
            from: query.from,
            to: query.to,
            review: query.review,
            error,
            saved_views,
            retried_count: query.retried_count,
            retry_failed_count: query.retry_failed_count,
            review_saved: query.review_saved,
            review_failed: query.review_failed,
            outcome_metrics,
            review_metrics,
            review_matrix,
            trend,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

/// GET /actions/export.csv — exports the complete filtered immutable invocation ledger.
pub async fn get_actions_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActionsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable")
            .into_response();
    };
    let (types, invocations, objects, reviews) = tokio::join!(
        client.list_action_types(&session.bearer_token),
        client.list_action_invocations(&session.bearer_token),
        client.list_objects(&session.bearer_token, None),
        client.list_action_reviews(&session.bearer_token),
    );
    let action_names = types
        .unwrap_or_default()
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let object_labels = objects
        .unwrap_or_default()
        .into_iter()
        .map(|object| (object.id, object_label(&object.properties, &object.id.to_string())))
        .collect::<std::collections::HashMap<_, _>>();
    let search = query.q.trim().to_ascii_lowercase();
    let action_filter = query.action.trim().to_ascii_lowercase();
    let (from, to) = parse_date_range(&query.from, &query.to);
    let invocations = match invocations {
        Ok(invocations) => invocations,
        Err(error) => {
            return (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    };
    let review_by_invocation = reviews
        .unwrap_or_default()
        .into_iter()
        .map(|review| (review.invocation_id, review))
        .collect::<std::collections::HashMap<_, _>>();
    let mut csv = String::from("id,action,outcome,targets,parameters,audit_context,executed_at\n");
    for invocation in invocations {
        let outcome_matches = match query.outcome.as_str() {
            "completed" => invocation.outcome.eq_ignore_ascii_case("completed"),
            "review" => !invocation.outcome.eq_ignore_ascii_case("completed"),
            _ => true,
        };
        let target_ids = invocation
            .target_object_ids
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        let targets = target_ids
            .iter()
            .map(|id| {
                object_labels
                    .get(&id.parse::<Uuid>().unwrap_or_default())
                    .cloned()
                    .unwrap_or_else(|| (*id).to_string())
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let action = action_names
            .get(&invocation.action_type_id)
            .cloned()
            .unwrap_or_else(|| "Unknown action".to_string());
        let review_status = review_by_invocation
            .get(&invocation.id)
            .map(|review| review.status.replace('_', " "))
            .unwrap_or_else(|| "not reviewed".to_string());
        let review_assignee = review_by_invocation
            .get(&invocation.id)
            .and_then(|review| review.assignee.clone())
            .unwrap_or_default();
        let parameters = invocation.parameters.to_string();
        let audit_context = invocation.triggering_event_ref.to_string();
        let haystack =
            format!("{action} {targets} {parameters} {audit_context}").to_ascii_lowercase();
        let action_matches = action_matches_filter(&action, &action_filter);
        let date_matches = from.map(|value| invocation.executed_at >= value).unwrap_or(true)
            && to.map(|value| invocation.executed_at <= value).unwrap_or(true);
        let review_stale = review_by_invocation
            .get(&invocation.id)
            .map(|review| review_is_stale(review.due_at, &review.status, Utc::now()))
            .unwrap_or(false);
        let review_matches =
            review_matches_filter(&review_status, &review_assignee, review_stale, &query.review);
        if outcome_matches
            && action_matches
            && review_matches
            && date_matches
            && (search.is_empty() || haystack.contains(&search))
        {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                invocation.id,
                csv_escape(&action),
                csv_escape(&invocation.outcome),
                csv_escape(&targets),
                csv_escape(&parameters),
                csv_escape(&audit_context),
                invocation.executed_at.to_rfc3339()
            ));
        }
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"action-ledger-{}.csv\"", session.tenant_id)
            .parse()
            .unwrap(),
    );
    (response_headers, csv).into_response()
}

#[cfg(test)]
mod tests {
    use super::{
        action_matches_filter, action_matches_search, action_view_redirect, bulk_retry_redirect,
        bulk_review_redirect, csv_escape, parse_date_range, review_matches_filter,
    };
    use axum::response::IntoResponse;

    #[test]
    fn csv_escape_preserves_action_evidence_with_delimiters() {
        assert_eq!(csv_escape("needs, review"), "\"needs, review\"");
        assert_eq!(csv_escape("{\"status\":\"open\"}"), "\"{\"\"status\"\":\"\"open\"\"}\"");
    }

    #[test]
    fn action_filter_is_case_insensitive_and_exact() {
        assert!(action_matches_filter("Escalate support ticket", " escalate SUPPORT ticket "));
        assert!(!action_matches_filter("Escalate support ticket", "support"));
    }

    #[test]
    fn date_range_is_inclusive_for_action_execution_days() {
        let (from, to) = parse_date_range("2026-07-15", "2026-07-20");
        assert_eq!(from.unwrap().to_rfc3339(), "2026-07-15T00:00:00+00:00");
        assert_eq!(to.unwrap().to_rfc3339(), "2026-07-20T23:59:59+00:00");
    }

    #[test]
    fn action_search_matches_invocation_and_target_ids() {
        let invocation_id = uuid::Uuid::new_v4();
        let target_id = uuid::Uuid::new_v4();
        assert!(action_matches_search(
            invocation_id,
            "Escalate ticket",
            &target_id.to_string(),
            "{\"priority\":\"high\"}",
            "{\"source\":\"console\"}",
            &target_id.to_string(),
        ));
        assert!(action_matches_search(
            invocation_id,
            "Escalate ticket",
            &target_id.to_string(),
            "{}",
            "{}",
            &invocation_id.to_string(),
        ));
    }

    #[test]
    fn review_filter_matches_handoff_and_assignment_state() {
        assert!(review_matches_filter("handed off", "operator", false, "handed_off"));
        assert!(review_matches_filter("open", "operator", false, "assigned"));
        assert!(!review_matches_filter("not reviewed", "", false, "assigned"));
        assert!(review_matches_filter("open", "", true, "stale"));
        assert!(review_matches_filter("anything", "", false, ""));
    }

    #[test]
    fn action_ledger_exposes_human_review_posture() {
        let template = include_str!("../templates/actions.html");
        assert!(template.contains("Human review posture"));
        assert!(template.contains("invocation.review_status"));
        assert!(template.contains("invocation.review_stale"));
        assert!(template.contains("/actions/{{ invocation.id }}"));
        assert!(template.contains("Bulk review transition"));
        assert!(template.contains("/actions/bulk-review"));
    }

    #[test]
    fn bulk_review_exposes_transition_preflight() {
        let template = include_str!("../templates/actions.html");
        assert!(template.contains("bulk-review-preflight"));
        assert!(template.contains("Immutable outcomes remain unchanged"));
        assert!(template.contains("assignee:"));
    }

    #[test]
    fn action_execution_exposes_target_preflight_boundary() {
        let template = include_str!("../templates/actions.html");
        assert!(template.contains("data-action-execution-preflight"));
        assert!(template.contains("visible contract gate"));
        assert!(template.contains("No state changes occur until you submit."));
    }

    #[test]
    fn bulk_retry_redirect_preserves_review_scope() {
        let fields = vec![
            ("q".to_string(), "northwind".to_string()),
            ("outcome".to_string(), "review".to_string()),
            ("action".to_string(), "Escalate support ticket".to_string()),
            ("from".to_string(), "2026-07-01".to_string()),
            ("to".to_string(), "2026-07-23".to_string()),
        ];
        let location = bulk_retry_redirect(&fields, "bulk-retry-complete", 2, 1)
            .into_response()
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(location.contains("q=northwind"));
        assert!(location.contains("outcome=review"));
        assert!(location.contains("from=2026-07-01"));
        assert!(location.contains("notice=bulk-retry-complete"));
    }

    #[test]
    fn bulk_review_redirect_preserves_scope_and_reports_counts() {
        let fields = vec![
            ("q".to_string(), "northwind".to_string()),
            ("outcome".to_string(), "review".to_string()),
            ("action".to_string(), "Escalate support ticket".to_string()),
        ];
        let response = bulk_review_redirect(&fields, "bulk-review-complete", "handed_off", 2, 1)
            .into_response();
        let location = response.headers().get("location").unwrap().to_str().unwrap();
        assert!(location.contains("review=handed_off"));
        assert!(location.contains("review_saved=2"));
        assert!(location.contains("review_failed=1"));
    }

    #[test]
    fn action_view_redirect_preserves_review_scope() {
        let form = super::SaveActionViewForm {
            name: "Needs review".to_string(),
            q: "Northwind".to_string(),
            outcome: "review".to_string(),
            action: "Escalate support ticket".to_string(),
            from: "2026-07-01".to_string(),
            to: "2026-07-23".to_string(),
            review: "handed_off".to_string(),
        };
        let location = action_view_redirect(&form, "view_saved")
            .into_response()
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(location.contains("q=Northwind"));
        assert!(location.contains("action=Escalate+support+ticket"));
        assert!(location.contains("notice=view_saved"));
    }
}
