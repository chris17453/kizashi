#[path = "actions_library_handler_test.rs"]
#[cfg(test)]
mod actions_library_handler_test;

use crate::{ontology_client, session_guard::require_session, AppState, CreateActionTypeRequest};
use askama::Template;
use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use common::ontology::{ActionInvocation, ActionType, ActionTypeHistory};
use uuid::Uuid;

#[derive(Debug, serde::Deserialize, Default)]
pub struct ActionLibraryQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub notice: String,
}

struct ActionTypeView {
    id: Uuid,
    name: String,
    target_type_id: Option<Uuid>,
    target_type: String,
    parameter_schema: String,
    preconditions: String,
    effect_definition: String,
    parameter_fields: Vec<ActionParameterView>,
    target_options: Vec<ActionTargetView>,
    has_eligible_target: bool,
    target_count: usize,
    eligible_target_count: usize,
    completed_invocation_count: usize,
    review_invocation_count: usize,
    invocation_count: usize,
    last_outcome: String,
    created_at: String,
    updated_at: String,
    history: Vec<ActionHistoryView>,
    superseded_invocations: usize,
}

struct ActionHistoryView {
    change_type: String,
    actor: String,
    changed_at: String,
    before_state: String,
    after_state: String,
}

struct ObjectTypeOption {
    id: Uuid,
    name: String,
}

struct ActionTargetCoverage {
    target_type: String,
    action_count: usize,
    eligible_count: usize,
    blocked_count: usize,
    percent: i32,
}

struct ActionParameterView {
    name: String,
    field_type: String,
    required: bool,
    default_value: String,
}

#[derive(Clone)]
struct ActionTargetView {
    id: Uuid,
    label: String,
    eligible: bool,
}

#[derive(Template)]
#[template(path = "actions_library.html")]
struct ActionLibraryTemplate {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    query: String,
    notice: String,
    action_types: Vec<ActionTypeView>,
    object_types: Vec<ObjectTypeOption>,
    error: Option<String>,
    eligible_contract_count: usize,
    blocked_contract_count: usize,
    execution_count: usize,
    superseded_invocation_count: usize,
    target_coverage: Vec<ActionTargetCoverage>,
}

fn action_library_query(value: &str) -> String {
    value.trim().to_string()
}

fn action_target_coverage(actions: &[ActionTypeView]) -> Vec<ActionTargetCoverage> {
    let mut counts = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for action in actions {
        let entry = counts.entry(action.target_type.clone()).or_default();
        entry.0 += 1;
        if action.has_eligible_target {
            entry.1 += 1;
        }
    }
    counts
        .into_iter()
        .map(|(target_type, (action_count, eligible_count))| ActionTargetCoverage {
            target_type,
            action_count,
            eligible_count,
            blocked_count: action_count.saturating_sub(eligible_count),
            percent: if action_count == 0 {
                0
            } else {
                ((eligible_count * 100) / action_count) as i32
            },
        })
        .collect()
}

fn matches_action_query(name: &str, target_type: &str, contract: &str, query: &str) -> bool {
    let query = action_library_query(query).to_ascii_lowercase();
    query.is_empty()
        || format!("{name} {target_type} {contract}").to_ascii_lowercase().contains(&query)
}

fn target_type_name(
    action: &ActionType,
    names: &std::collections::HashMap<Uuid, String>,
) -> String {
    action
        .target_object_type_id
        .and_then(|id| names.get(&id).cloned())
        .unwrap_or_else(|| "Any object type".to_string())
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

fn parameter_fields(schema: &serde_json::Value) -> Vec<ActionParameterView> {
    let Some(fields) = schema.as_object() else {
        return Vec::new();
    };
    fields
        .iter()
        .map(|(name, definition)| {
            let field_type = definition
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string")
                .to_string();
            let default_value = definition
                .get("default")
                .map(|value| {
                    if field_type == "string" {
                        value.as_str().unwrap_or_default().to_string()
                    } else {
                        value.to_string()
                    }
                })
                .unwrap_or_else(|| match field_type.as_str() {
                    "boolean" => "false".to_string(),
                    "array" => "[]".to_string(),
                    "object" => "{}".to_string(),
                    _ => String::new(),
                });
            ActionParameterView {
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

fn object_label(object: &common::ontology::Object) -> String {
    object
        .properties
        .get("name")
        .or_else(|| object.properties.get("title"))
        .or_else(|| object.properties.get("subject"))
        .or_else(|| object.properties.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Untitled object")
        .to_string()
}

fn view_action_type(
    action: ActionType,
    target_names: &std::collections::HashMap<Uuid, String>,
    invocations: &[ActionInvocation],
    objects: &[common::ontology::Object],
) -> ActionTypeView {
    let target_type = target_type_name(&action, target_names);
    let target_type_id = action.target_object_type_id;
    let target_options: Vec<ActionTargetView> = objects
        .iter()
        .filter(|object| target_type_id.is_none() || Some(object.object_type_id) == target_type_id)
        .map(|object| ActionTargetView {
            id: object.id,
            label: object_label(object),
            eligible: satisfies_preconditions(&object.properties, &action.preconditions),
        })
        .collect();
    let has_eligible_target = target_options.iter().any(|target| target.eligible);
    let target_count = target_options.len();
    let eligible_target_count = target_options.iter().filter(|target| target.eligible).count();
    let related = invocations
        .iter()
        .filter(|invocation| invocation.action_type_id == action.id)
        .collect::<Vec<_>>();
    let last_outcome = related
        .first()
        .map(|invocation| invocation.outcome.clone())
        .unwrap_or_else(|| "No executions".to_string());
    let completed_invocation_count = related
        .iter()
        .filter(|invocation| invocation.outcome.eq_ignore_ascii_case("completed"))
        .count();
    let review_invocation_count = related.len().saturating_sub(completed_invocation_count);
    let current_target_type =
        action.target_object_type_id.map(|id| serde_json::Value::String(id.to_string()));
    let superseded_invocations = related
        .iter()
        .filter(|invocation| {
            let Some(snapshot) =
                invocation.contract_snapshot.as_ref().and_then(serde_json::Value::as_object)
            else {
                return false;
            };
            snapshot.get("name") != Some(&serde_json::Value::String(action.name.clone()))
                || snapshot.get("target_object_type_id") != current_target_type.as_ref()
                || snapshot.get("parameter_schema") != Some(&action.parameter_schema)
                || snapshot.get("preconditions") != Some(&action.preconditions)
                || snapshot.get("effect_definition") != Some(&action.effect_definition)
        })
        .count();
    ActionTypeView {
        id: action.id,
        name: action.name,
        target_type_id,
        target_type,
        parameter_schema: serde_json::to_string_pretty(&action.parameter_schema)
            .unwrap_or_else(|_| "{}".to_string()),
        preconditions: serde_json::to_string_pretty(&action.preconditions)
            .unwrap_or_else(|_| "{}".to_string()),
        effect_definition: serde_json::to_string_pretty(&action.effect_definition)
            .unwrap_or_else(|_| "{}".to_string()),
        parameter_fields: parameter_fields(&action.parameter_schema),
        target_options,
        has_eligible_target,
        target_count,
        eligible_target_count,
        completed_invocation_count,
        review_invocation_count,
        invocation_count: related.len(),
        last_outcome,
        created_at: action.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        updated_at: action.updated_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        history: Vec::new(),
        superseded_invocations,
    }
}

async fn load_action_library(
    token: &str,
    query: &str,
) -> (Vec<ActionTypeView>, Vec<ObjectTypeOption>, Option<String>) {
    let Some(client) = ontology_client::global() else {
        return (Vec::new(), Vec::new(), Some("Ontology client unavailable".to_string()));
    };
    let (actions, object_types, invocations, objects) = tokio::join!(
        client.list_action_types(token),
        client.list_object_types(token),
        client.list_action_invocations(token),
        client.list_objects(token, None)
    );
    let mut error = None;
    let object_types = match object_types {
        Ok(items) => items,
        Err(e) => {
            error = Some(format!("object types: {e}"));
            Vec::new()
        }
    };
    let target_names = object_types
        .iter()
        .map(|item| (item.id, item.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let invocations = match invocations {
        Ok(items) => items,
        Err(e) => {
            if error.is_none() {
                error = Some(format!("action ledger: {e}"));
            }
            Vec::new()
        }
    };
    let objects = match objects {
        Ok(items) => items,
        Err(e) => {
            if error.is_none() {
                error = Some(format!("ontology objects: {e}"));
            }
            Vec::new()
        }
    };
    let action_types = match actions {
        Ok(items) => items,
        Err(e) => {
            return (
                Vec::new(),
                object_types
                    .into_iter()
                    .map(|item| ObjectTypeOption { id: item.id, name: item.name })
                    .collect(),
                Some(format!("action definitions: {e}")),
            );
        }
    };
    let mut views = action_types
        .into_iter()
        .map(|item| view_action_type(item, &target_names, &invocations, &objects))
        .filter(|item| {
            matches_action_query(
                &item.name,
                &item.target_type,
                &format!(
                    "{} {} {}",
                    item.parameter_schema, item.preconditions, item.effect_definition
                ),
                query,
            )
        })
        .collect::<Vec<_>>();
    for view in &mut views {
        if let Ok(history) = client.list_action_type_history(token, view.id).await {
            view.history = history.into_iter().map(action_history_view).collect();
        }
    }
    views.sort_by(|left, right| {
        left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase())
    });
    (
        views,
        object_types
            .into_iter()
            .map(|item| ObjectTypeOption { id: item.id, name: item.name })
            .collect(),
        error,
    )
}

fn action_history_view(history: ActionTypeHistory) -> ActionHistoryView {
    ActionHistoryView {
        change_type: history.change_type,
        actor: history.actor,
        changed_at: history.changed_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        before_state: serde_json::to_string_pretty(&history.before_state)
            .unwrap_or_else(|_| "null".to_string()),
        after_state: serde_json::to_string_pretty(&history.after_state)
            .unwrap_or_else(|_| "null".to_string()),
    }
}

pub async fn get_action_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ActionLibraryQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let query_text = action_library_query(&query.q);
    let (action_types, object_types, error) =
        load_action_library(&session.bearer_token, &query_text).await;
    let eligible_contract_count =
        action_types.iter().filter(|action| action.has_eligible_target).count();
    let blocked_contract_count = action_types.len().saturating_sub(eligible_contract_count);
    let execution_count = action_types.iter().map(|action| action.invocation_count).sum();
    let superseded_invocation_count =
        action_types.iter().map(|action| action.superseded_invocations).sum();
    let target_coverage = action_target_coverage(&action_types);
    Html(
        ActionLibraryTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            can_manage: session.role.at_least(common::Role::Operator),
            query: query_text,
            notice: query.notice,
            action_types,
            object_types,
            error,
            eligible_contract_count,
            blocked_contract_count,
            execution_count,
            superseded_invocation_count,
            target_coverage,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct ActionLibraryForm {
    name: String,
    target_object_type_id: Option<Uuid>,
    parameter_schema: String,
    preconditions: String,
    effect_definition: String,
}

fn parse_contracts(form: &ActionLibraryForm) -> Result<CreateActionTypeRequest, Response> {
    let parse = |name: &str, value: &str| {
        serde_json::from_str(value).map_err(|_| {
            (StatusCode::BAD_REQUEST, format!("{name} must be valid JSON")).into_response()
        })
    };
    Ok(CreateActionTypeRequest {
        name: form.name.trim().to_string(),
        target_object_type_id: form.target_object_type_id,
        parameter_schema: parse("Parameter schema", &form.parameter_schema)?,
        preconditions: parse("Preconditions", &form.preconditions)?,
        effect_definition: parse("Effect definition", &form.effect_definition)?,
    })
}

pub async fn post_create_action_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ActionLibraryForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let input = match parse_contracts(&form) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.create_action_type(&session.bearer_token, &input).await {
        Ok(()) => Redirect::to("/actions/library?notice=created").into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

pub async fn post_update_action_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionLibraryForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let input = match parse_contracts(&form) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.update_action_type(&session.bearer_token, id, &input).await {
        Ok(()) => Redirect::to("/actions/library?notice=updated").into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

pub async fn post_delete_action_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.delete_action_type(&session.bearer_token, id).await {
        Ok(()) => Redirect::to("/actions/library?notice=deleted").into_response(),
        Err(error) => (StatusCode::CONFLICT, error.to_string()).into_response(),
    }
}
