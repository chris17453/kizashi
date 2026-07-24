#[path = "ontology_handler_test.rs"]
#[cfg(test)]
mod ontology_handler_test;

use askama::Template;
use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use common::SavedSearchQuery;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ontology_client, session_guard::require_session, AppState, CreateObjectRequest,
    CreateObjectTypeRequest, InvokeActionRequest,
};
use crate::{CreateActionTypeRequest, CreateLinkRequest, CreateLinkTypeRequest};

#[derive(Template)]
#[template(path = "ontology.html")]
struct OntologyTemplate {
    show_nav: bool,
    is_admin: bool,
    can_manage: bool,
    object_types: Vec<ObjectTypeView>,
    link_types: Vec<LinkTypeView>,
    links: Vec<LinkInstanceView>,
    graph_nodes: Vec<GraphNodeView>,
    graph_edges: Vec<GraphEdgeView>,
    graph_total_count: usize,
    graph_depth: usize,
    path_from: Option<Uuid>,
    path_to: Option<Uuid>,
    graph_path: Vec<GraphPathNode>,
    objects: Vec<ObjectView>,
    matching_count: usize,
    page: i64,
    has_prev: bool,
    has_next: bool,
    relationship_objects: Vec<RelationshipObjectView>,
    action_invocations: Vec<ActionInvocationView>,
    action_types: Vec<OntologyActionTypeView>,
    selected_type: Option<Uuid>,
    selected_object: Option<Uuid>,
    q: String,
    property: String,
    value: String,
    risk: String,
    error: Option<String>,
    saved_views: Vec<SavedOntologyView>,
    notice: String,
    created_count: usize,
    failed_count: usize,
    link_type_filter: Option<Uuid>,
    matching_link_count: usize,
    relationship_matrix_types: Vec<String>,
    relationship_matrix: Vec<RelationshipMatrixRow>,
    property_coverage_fields: Vec<String>,
    property_coverage: Vec<PropertyCoverageRow>,
    risk_metrics: Vec<ObjectRiskMetric>,
}

const ONTOLOGY_PAGE_SIZE: usize = 25;

struct ObjectTypeView {
    id: Uuid,
    name: String,
    version: i32,
    property_count: usize,
    mapping_count: usize,
    link_type_count: usize,
    object_count: usize,
    selected: bool,
    schema: String,
    mapping: String,
    object_percent: i32,
    property_percent: i32,
    mapping_percent: i32,
    link_percent: i32,
}
struct PropertyView {
    key: String,
    value: String,
}
struct PropertyEditorField {
    key: String,
    input_type: String,
    value: String,
    checked: bool,
    required: bool,
}
struct ObjectView {
    id: Uuid,
    object_type_id: Uuid,
    type_name: String,
    summary: String,
    properties: String,
    property_rows: Vec<PropertyView>,
    editor_fields: Vec<PropertyEditorField>,
    lineage: String,
    lineage_ids: Vec<Uuid>,
    related: Vec<RelatedObjectView>,
    signals: Vec<ObjectSignalView>,
    incidents: Vec<ObjectIncidentView>,
    history: Vec<ObjectHistoryView>,
    activity: Vec<ObjectActivityView>,
    risk_score: usize,
    risk_label: String,
    risk_tone: String,
    risk_reasons: Vec<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
}
struct ObjectSignalView {
    id: Uuid,
    event_type: String,
    status: String,
    occurred_at: chrono::DateTime<chrono::Utc>,
}
struct ObjectIncidentView {
    id: Uuid,
    title: String,
    severity: String,
    status: String,
}
struct ObjectHistoryView {
    change_type: String,
    actor: String,
    before_state: String,
    after_state: String,
    changed_at: chrono::DateTime<chrono::Utc>,
}
struct RelatedObjectView {
    relationship: String,
    id: Uuid,
    type_name: String,
    summary: String,
}
struct ObjectActivityView {
    id: Uuid,
    action_name: String,
    outcome: String,
    review_status: String,
    review_assignee: Option<String>,
    review_stale: bool,
    executed_at: chrono::DateTime<chrono::Utc>,
    event_id: Option<Uuid>,
}

struct ObjectRiskMetric {
    key: String,
    label: String,
    count: usize,
    percent: i32,
}

fn object_risk_posture(
    signals: &[ObjectSignalView],
    incidents: &[ObjectIncidentView],
    activity: &[ObjectActivityView],
) -> (usize, String, String, Vec<String>) {
    let mut score = 0usize;
    let mut reasons = Vec::new();
    for incident in incidents.iter().filter(|item| item.status != "resolved") {
        let (weight, label) = match incident.severity.as_str() {
            "critical" => (65, "Critical case"),
            "high" => (45, "High-severity case"),
            "medium" => (25, "Medium-severity case"),
            _ => (12, "Open case"),
        };
        score += weight;
        reasons.push(format!("{label}: {}", incident.title));
    }
    for signal in signals {
        let weight = match signal.status.as_str() {
            "triggered" => 18,
            "new" => 10,
            _ => 4,
        };
        score += weight;
        if reasons.len() < 4 {
            reasons.push(format!("Signal: {} ({})", signal.event_type, signal.status));
        }
    }
    for action in activity.iter().filter(|item| item.outcome != "Completed") {
        score += if action.review_stale { 15 } else { 8 };
        if reasons.len() < 4 {
            reasons.push(format!(
                "Decision review: {}{}",
                action.action_name,
                if action.review_stale { " (overdue)" } else { "" }
            ));
        }
    }
    let score = score.min(100);
    let (label, tone) = match score {
        0 => ("Stable", "good"),
        1..=25 => ("Monitored", "neutral"),
        26..=59 => ("Needs attention", "warning"),
        _ => ("Critical attention", "critical"),
    };
    (score, label.to_string(), tone.to_string(), reasons)
}
struct RelationshipObjectView {
    id: Uuid,
    object_type_id: Uuid,
    type_name: String,
    summary: String,
}
struct LinkTypeView {
    id: Uuid,
    name: String,
    cardinality: String,
    source_id: Uuid,
    target_id: Uuid,
    source_name: String,
    target_name: String,
    selected: bool,
    instance_count: usize,
}

struct RelationshipMatrixCell {
    count: usize,
    href: String,
}

struct RelationshipMatrixRow {
    source_name: String,
    cells: Vec<RelationshipMatrixCell>,
}

struct PropertyCoverageCell {
    present: usize,
    percent: i32,
    tone: String,
}

struct PropertyCoverageRow {
    type_id: Uuid,
    type_name: String,
    object_count: usize,
    cells: Vec<PropertyCoverageCell>,
}

fn property_coverage(
    types: &[common::ontology::ObjectType],
    objects: &[common::ontology::Object],
) -> (Vec<String>, Vec<PropertyCoverageRow>) {
    let mut field_frequency = std::collections::HashMap::<String, usize>::new();
    for object_type in types {
        if let Some(schema) = object_type.property_schema.as_object() {
            for field in schema.keys() {
                *field_frequency.entry(field.clone()).or_default() += 1;
            }
        }
    }
    // Keep the matrix readable while favoring fields that are part of the most schemas.
    let mut fields = field_frequency.into_iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    fields.truncate(14);
    let mut fields = fields.into_iter().map(|(field, _)| field).collect::<Vec<_>>();
    fields.sort();
    let rows = types
        .iter()
        .map(|object_type| {
            let typed_objects = objects
                .iter()
                .filter(|object| object.object_type_id == object_type.id)
                .collect::<Vec<_>>();
            let total = typed_objects.len();
            let cells = fields
                .iter()
                .map(|field| {
                    let present = typed_objects
                        .iter()
                        .filter(|object| {
                            object
                                .properties
                                .get(field)
                                .map(|value| !value.is_null() && value != "")
                                .unwrap_or(false)
                        })
                        .count();
                    let percent = if total == 0 { 0 } else { ((present * 100) / total) as i32 };
                    let tone = if percent >= 90 {
                        "good"
                    } else if percent >= 60 {
                        "warning"
                    } else {
                        "danger"
                    };
                    PropertyCoverageCell { present, percent, tone: tone.to_string() }
                })
                .collect();
            PropertyCoverageRow {
                type_id: object_type.id,
                type_name: object_type.name.clone(),
                object_count: total,
                cells,
            }
        })
        .collect();
    (fields, rows)
}

fn relationship_matrix(
    types: &[common::ontology::ObjectType],
    link_types: &[common::ontology::LinkType],
    links: &[common::ontology::Link],
) -> (Vec<String>, Vec<RelationshipMatrixRow>) {
    let names = types.iter().map(|item| item.name.clone()).collect::<Vec<_>>();
    let mut counts = std::collections::HashMap::<(Uuid, Uuid), usize>::new();
    let mut first_link_type = std::collections::HashMap::<(Uuid, Uuid), Uuid>::new();
    for link in links {
        let Some(link_type) = link_types.iter().find(|item| item.id == link.link_type_id) else {
            continue;
        };
        let key = (link_type.source_object_type_id, link_type.target_object_type_id);
        *counts.entry(key).or_default() += 1;
        first_link_type.entry(key).or_insert(link_type.id);
    }
    let rows = types
        .iter()
        .map(|source| RelationshipMatrixRow {
            source_name: source.name.clone(),
            cells: types
                .iter()
                .map(|target| {
                    let key = (source.id, target.id);
                    RelationshipMatrixCell {
                        count: counts.get(&key).copied().unwrap_or(0),
                        href: first_link_type
                            .get(&key)
                            .map(|id| format!("/ontology?link_type_id={id}"))
                            .unwrap_or_else(|| "/ontology".to_string()),
                    }
                })
                .collect(),
        })
        .collect();
    (names, rows)
}
struct LinkInstanceView {
    id: Uuid,
    link_type_id: Uuid,
    link_type_name: String,
    source_id: Uuid,
    source_summary: String,
    target_id: Uuid,
    target_summary: String,
    properties: String,
}
struct GraphNodeView {
    id: Uuid,
    type_name: String,
    summary: String,
    x: i32,
    y: i32,
}
struct GraphEdgeView {
    source_id: Uuid,
    target_id: Uuid,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    label: String,
}
struct GraphPathNode {
    id: Uuid,
    type_name: String,
    summary: String,
}
struct ActionInvocationView {
    action_name: String,
    outcome: String,
    target_count: usize,
    executed_at: chrono::DateTime<chrono::Utc>,
}
struct OntologyActionParameterField {
    name: String,
    field_type: String,
    required: bool,
    default_value: String,
}
struct OntologyActionTargetOption {
    id: Uuid,
    label: String,
    type_name: String,
    eligible: bool,
}
#[derive(Debug, Serialize, Deserialize, Default)]
struct SavedOntologyFilter {
    #[serde(default)]
    type_id: Option<Uuid>,
    #[serde(default)]
    q: String,
    #[serde(default)]
    property: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    risk: String,
    #[serde(default)]
    link_type_id: Option<Uuid>,
}
struct SavedOntologyView {
    id: Uuid,
    name: String,
    load_url: String,
}

#[derive(Template)]
#[template(path = "ontology_compare.html")]
struct OntologyCompareTemplate {
    show_nav: bool,
    is_admin: bool,
    objects: Vec<CompareObjectView>,
    property_rows: Vec<ComparePropertyRow>,
    differing_property_count: usize,
    shared_property_count: usize,
}

struct CompareObjectView {
    id: Uuid,
    type_name: String,
    summary: String,
    updated_at: chrono::DateTime<chrono::Utc>,
    properties: serde_json::Map<String, serde_json::Value>,
}

struct ComparePropertyRow {
    key: String,
    values: Vec<String>,
}

fn to_saved_ontology_view(query: SavedSearchQuery) -> SavedOntologyView {
    let filter: SavedOntologyFilter = serde_json::from_value(query.filter).unwrap_or_default();
    let mut params = Vec::new();
    if let Some(type_id) = filter.type_id {
        params.push(("type_id", type_id.to_string()));
    }
    if !filter.q.is_empty() {
        params.push(("q", filter.q));
    }
    if !filter.property.is_empty() {
        params.push(("property", filter.property));
    }
    if !filter.value.is_empty() {
        params.push(("value", filter.value));
    }
    if !filter.risk.is_empty() {
        params.push(("risk", filter.risk));
    }
    if let Some(link_type_id) = filter.link_type_id {
        params.push(("link_type_id", link_type_id.to_string()));
    }
    let load_url = format!(
        "/ontology{}",
        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", serde_urlencoded::to_string(params).unwrap_or_default())
        }
    );
    SavedOntologyView { id: query.id, name: query.name, load_url }
}
struct OntologyActionTypeView {
    id: Uuid,
    name: String,
    target_object_type_id: Option<Uuid>,
    target_type_name: String,
    parameter_schema: String,
    parameter_fields: Vec<OntologyActionParameterField>,
    target_options: Vec<OntologyActionTargetOption>,
    preconditions: String,
    effect_definition: String,
}

fn object_matches_filter(
    object: &common::ontology::Object,
    text_query: &str,
    property_query: &str,
    value_query: &str,
) -> bool {
    let text_matches = text_query.is_empty()
        || object.id.to_string().contains(text_query)
        || object.object_type_id.to_string().contains(text_query)
        || object.properties.to_string().to_ascii_lowercase().contains(text_query);
    if !text_matches {
        return false;
    }
    if property_query.is_empty() && value_query.is_empty() {
        return true;
    }
    let Some(properties) = object.properties.as_object() else {
        return false;
    };
    properties.iter().any(|(key, value)| {
        (property_query.is_empty() || key.to_ascii_lowercase().contains(property_query))
            && (value_query.is_empty()
                || value.to_string().to_ascii_lowercase().trim_matches('"').contains(value_query))
    })
}

fn graph_neighborhood(
    objects: &[common::ontology::Object],
    links: &[common::ontology::Link],
    selected: Option<Uuid>,
    depth: usize,
    limit: usize,
) -> Vec<Uuid> {
    let Some(selected) = selected else {
        return Vec::new();
    };
    if !objects.iter().any(|object| object.id == selected) {
        return Vec::new();
    }
    let mut ordered = vec![selected];
    let mut seen = std::collections::HashSet::from([selected]);
    let mut frontier = vec![selected];
    for _ in 0..depth.max(1) {
        let mut next = Vec::new();
        for link in links {
            let neighbor = if frontier.contains(&link.source_object_id) {
                Some(link.target_object_id)
            } else if frontier.contains(&link.target_object_id) {
                Some(link.source_object_id)
            } else {
                None
            };
            let Some(neighbor) = neighbor else { continue };
            if seen.insert(neighbor) && objects.iter().any(|object| object.id == neighbor) {
                ordered.push(neighbor);
                next.push(neighbor);
                if ordered.len() >= limit {
                    return ordered;
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    ordered
}

fn shortest_object_path(
    objects: &[common::ontology::Object],
    links: &[common::ontology::Link],
    from: Option<Uuid>,
    to: Option<Uuid>,
) -> Vec<Uuid> {
    let (Some(from), Some(to)) = (from, to) else { return Vec::new() };
    if from == to && objects.iter().any(|object| object.id == from) {
        return vec![from];
    }
    let object_ids =
        objects.iter().map(|object| object.id).collect::<std::collections::HashSet<_>>();
    if !object_ids.contains(&from) || !object_ids.contains(&to) {
        return Vec::new();
    }
    let mut queue = std::collections::VecDeque::from([from]);
    let mut parent = std::collections::HashMap::<Uuid, Option<Uuid>>::from([(from, None)]);
    while let Some(current) = queue.pop_front() {
        for link in links {
            let neighbor = if link.source_object_id == current {
                link.target_object_id
            } else if link.target_object_id == current {
                link.source_object_id
            } else {
                continue;
            };
            if object_ids.contains(&neighbor) && parent.insert(neighbor, Some(current)).is_none() {
                if neighbor == to {
                    let mut path = vec![to];
                    let mut cursor = to;
                    while let Some(Some(previous)) = parent.get(&cursor) {
                        path.push(*previous);
                        cursor = *previous;
                    }
                    path.reverse();
                    return path;
                }
                queue.push_back(neighbor);
            }
        }
    }
    Vec::new()
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn satisfies_action_preconditions(
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

fn object_operational_context(
    lineage_ids: &[Uuid],
    events: &[crate::EventSummary],
    incidents: &[crate::IncidentDetail],
) -> (Vec<ObjectSignalView>, Vec<ObjectIncidentView>) {
    let signal_ids = events
        .iter()
        .filter(|event| event.record_ids.iter().any(|record_id| lineage_ids.contains(record_id)))
        .collect::<Vec<_>>();
    let signals = signal_ids
        .iter()
        .take(8)
        .map(|event| ObjectSignalView {
            id: event.id,
            event_type: event.event_type.clone(),
            status: event.status.clone(),
            occurred_at: event.occurred_at,
        })
        .collect::<Vec<_>>();
    let event_ids =
        signal_ids.iter().map(|event| event.id).collect::<std::collections::HashSet<_>>();
    let incidents = incidents
        .iter()
        .filter(|incident| incident.event_ids.iter().any(|event_id| event_ids.contains(event_id)))
        .take(8)
        .map(|incident| ObjectIncidentView {
            id: incident.incident.id,
            title: incident.incident.title.clone(),
            severity: incident.incident.severity.to_string(),
            status: incident.incident.status.to_string(),
        })
        .collect::<Vec<_>>();
    (signals, incidents)
}

fn default_action_parameters(
    schema: &serde_json::Value,
    mut parameters: serde_json::Value,
) -> serde_json::Value {
    let Some(fields) = schema.as_object() else {
        return parameters;
    };
    let Some(values) = parameters.as_object_mut() else {
        return parameters;
    };
    for (name, definition) in fields {
        if values.contains_key(name) {
            continue;
        }
        // Optional fields may receive a type-correct default for the convenient ontology
        // authoring form. Required fields must remain absent so the ontology service can reject
        // the invocation instead of silently turning a missing value into an empty placeholder.
        if definition.get("required").and_then(serde_json::Value::as_bool).unwrap_or(true) {
            continue;
        }
        let value = match definition.get("type").and_then(serde_json::Value::as_str) {
            Some("boolean") => serde_json::Value::Bool(false),
            Some("number") | Some("integer") => serde_json::json!(0),
            Some("array") => serde_json::json!([]),
            Some("object") => serde_json::json!({}),
            _ => serde_json::Value::String(String::new()),
        };
        values.insert(name.clone(), value);
    }
    parameters
}

fn build_property_editor_fields(
    schema: &serde_json::Value,
    properties: &serde_json::Value,
) -> Vec<PropertyEditorField> {
    let Some(schema) = schema.as_object() else {
        return Vec::new();
    };
    let values = properties.as_object();
    let mut fields = schema
        .iter()
        .filter_map(|(key, definition)| {
            let definition = definition.as_object()?;
            let type_name =
                definition.get("type").and_then(serde_json::Value::as_str).unwrap_or("string");
            let input_type = match type_name {
                "boolean" => "boolean",
                "number" | "integer" => "number",
                "array" | "object" => "json",
                _ => "text",
            }
            .to_string();
            let value = values.and_then(|items| items.get(key));
            let checked = value.and_then(serde_json::Value::as_bool).unwrap_or(false);
            let rendered = match value {
                Some(value) if input_type == "json" => {
                    serde_json::to_string(value).unwrap_or_default()
                }
                Some(value) if value.is_string() => value.as_str().unwrap_or_default().to_string(),
                Some(value) => value.to_string(),
                None => String::new(),
            };
            Some(PropertyEditorField {
                key: key.clone(),
                input_type,
                value: rendered,
                checked,
                required: definition
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.key.cmp(&right.key));
    fields
}

fn build_action_parameter_fields(schema: &serde_json::Value) -> Vec<OntologyActionParameterField> {
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
            OntologyActionParameterField {
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

#[derive(Debug, Deserialize, Default)]
pub struct OntologyQuery {
    pub type_id: Option<Uuid>,
    pub object_id: Option<Uuid>,
    #[serde(default)]
    pub link_type_id: Option<Uuid>,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub property: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub page: i64,
    #[serde(default = "default_graph_depth")]
    pub depth: usize,
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub created_count: usize,
    #[serde(default)]
    pub failed_count: usize,
    #[serde(default)]
    pub path_from: Option<Uuid>,
    #[serde(default)]
    pub path_to: Option<Uuid>,
}

fn default_graph_depth() -> usize {
    1
}

#[derive(Debug, Deserialize)]
pub struct ObjectTypeForm {
    pub name: String,
    pub version: i32,
    pub property_schema: String,
    pub mapping_rules: String,
}

pub async fn list_ontology(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let token = session.bearer_token.as_str();
    let saved_views = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|query| {
            query.filter.get("surface").and_then(serde_json::Value::as_str) == Some("ontology")
        })
        .map(to_saved_ontology_view)
        .collect::<Vec<_>>();
    let client = match ontology_client::global() {
        Some(client) => client,
        None => {
            return Html(
                OntologyTemplate {
                    show_nav: true,
                    is_admin: session.role.at_least(common::Role::Admin),
                    can_manage: session.role.at_least(common::Role::Operator),
                    object_types: vec![],
                    link_types: vec![],
                    links: vec![],
                    graph_nodes: vec![],
                    graph_edges: vec![],
                    graph_total_count: 0,
                    graph_depth: 1,
                    path_from: None,
                    path_to: None,
                    graph_path: vec![],
                    objects: vec![],
                    matching_count: 0,
                    page: query.page.max(0),
                    has_prev: false,
                    has_next: false,
                    relationship_objects: vec![],
                    action_invocations: vec![],
                    selected_type: query.type_id,
                    selected_object: query.object_id,
                    action_types: vec![],
                    q: query.q.clone(),
                    property: query.property.clone(),
                    value: query.value.clone(),
                    risk: query.risk.clone(),
                    error: Some("Ontology client is not configured".to_string()),
                    saved_views,
                    notice: query.notice.clone(),
                    created_count: query.created_count,
                    failed_count: query.failed_count,
                    link_type_filter: query.link_type_id,
                    matching_link_count: 0,
                    relationship_matrix_types: vec![],
                    relationship_matrix: vec![],
                    property_coverage_fields: vec![],
                    property_coverage: vec![],
                    risk_metrics: vec![],
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    };

    let result = tokio::join!(
        client.list_object_types(token),
        client.list_link_types(token),
        client.list_objects(token, query.type_id),
        client.list_action_invocations(token),
        client.list_links(token),
    );
    let error = [
        &result.0.as_ref().err(),
        &result.1.as_ref().err(),
        &result.2.as_ref().err(),
        &result.3.as_ref().err(),
        &result.4.as_ref().err(),
    ]
    .into_iter()
    .flatten()
    .next()
    .map(ToString::to_string);

    let types = result.0.unwrap_or_default();
    let type_names: std::collections::HashMap<Uuid, String> =
        types.iter().map(|t| (t.id, t.name.clone())).collect();
    let all_objects_for_counts = client.list_objects(token, None).await.unwrap_or_default();
    let mut object_counts = std::collections::HashMap::<Uuid, usize>::new();
    for object in &all_objects_for_counts {
        *object_counts.entry(object.object_type_id).or_default() += 1;
    }
    let total_object_count = all_objects_for_counts.len().max(1);
    let link_types_for_counts = result.1.as_ref().map(|items| items.as_slice()).unwrap_or(&[]);
    let max_property_count = types
        .iter()
        .map(|item| item.property_schema.as_object().map(|object| object.len()).unwrap_or(0))
        .max()
        .unwrap_or(1);
    let max_mapping_count = types
        .iter()
        .map(|item| item.mapping_rules.as_array().map(|array| array.len()).unwrap_or(0))
        .max()
        .unwrap_or(1);
    let max_link_type_count = types
        .iter()
        .map(|item| {
            link_types_for_counts
                .iter()
                .filter(|link| {
                    link.source_object_type_id == item.id || link.target_object_type_id == item.id
                })
                .count()
        })
        .max()
        .unwrap_or(1);
    let object_types = types
        .iter()
        .map(|t| {
            let property_count = t.property_schema.as_object().map(|o| o.len()).unwrap_or(0);
            let mapping_count = t.mapping_rules.as_array().map(|a| a.len()).unwrap_or(0);
            let link_type_count = link_types_for_counts
                .iter()
                .filter(|link| {
                    link.source_object_type_id == t.id || link.target_object_type_id == t.id
                })
                .count();
            ObjectTypeView {
                id: t.id,
                name: t.name.clone(),
                version: t.version,
                property_count,
                mapping_count,
                link_type_count,
                object_count: object_counts.get(&t.id).copied().unwrap_or(0),
                selected: query.type_id == Some(t.id),
                schema: serde_json::to_string_pretty(&t.property_schema).unwrap_or_default(),
                mapping: serde_json::to_string_pretty(&t.mapping_rules).unwrap_or_default(),
                object_percent: ((object_counts.get(&t.id).copied().unwrap_or(0) * 100)
                    / total_object_count) as i32,
                property_percent: ((property_count * 100) / max_property_count) as i32,
                mapping_percent: ((mapping_count * 100) / max_mapping_count) as i32,
                link_percent: ((link_type_count * 100) / max_link_type_count) as i32,
            }
        })
        .collect();
    let relationship_objects = all_objects_for_counts
        .iter()
        .map(|object| RelationshipObjectView {
            id: object.id,
            object_type_id: object.object_type_id,
            type_name: type_names
                .get(&object.object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown type".to_string()),
            summary: object
                .properties
                .get("name")
                .or_else(|| object.properties.get("subject"))
                .or_else(|| object.properties.get("title"))
                .or_else(|| object.properties.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Untitled object")
                .to_string(),
        })
        .collect::<Vec<_>>();
    let action_types_raw = client.list_action_types(token).await.unwrap_or_default();
    let action_types = action_types_raw
        .iter()
        .map(|action| {
            let mut target_options = all_objects_for_counts
                .iter()
                .map(|object| OntologyActionTargetOption {
                    id: object.id,
                    label: object
                        .properties
                        .get("name")
                        .or_else(|| object.properties.get("subject"))
                        .or_else(|| object.properties.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Untitled object")
                        .to_string(),
                    type_name: type_names
                        .get(&object.object_type_id)
                        .cloned()
                        .unwrap_or_else(|| "Entity".to_string()),
                    eligible: action
                        .target_object_type_id
                        .map(|type_id| type_id == object.object_type_id)
                        .unwrap_or(true)
                        && satisfies_action_preconditions(
                            &object.properties,
                            &action.preconditions,
                        ),
                })
                .collect::<Vec<_>>();
            target_options.sort_by_key(|target| !target.eligible);
            OntologyActionTypeView {
                id: action.id,
                name: action.name.clone(),
                target_object_type_id: action.target_object_type_id,
                target_type_name: action
                    .target_object_type_id
                    .and_then(|id| type_names.get(&id).cloned())
                    .unwrap_or_else(|| "Any object type".to_string()),
                parameter_schema: serde_json::to_string_pretty(&action.parameter_schema)
                    .unwrap_or_default(),
                parameter_fields: build_action_parameter_fields(&action.parameter_schema),
                target_options,
                preconditions: serde_json::to_string_pretty(&action.preconditions)
                    .unwrap_or_default(),
                effect_definition: serde_json::to_string_pretty(&action.effect_definition)
                    .unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    let object_query = query.q.trim().to_ascii_lowercase();
    let property_query = query.property.trim().to_ascii_lowercase();
    let value_query = query.value.trim().to_ascii_lowercase();
    let all_objects = result.2.unwrap_or_default();
    let object_summaries: std::collections::HashMap<Uuid, String> = all_objects_for_counts
        .iter()
        .map(|object| {
            (
                object.id,
                object
                    .properties
                    .get("name")
                    .or_else(|| object.properties.get("subject"))
                    .or_else(|| object.properties.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled object")
                    .to_string(),
            )
        })
        .collect();
    let object_type_ids: std::collections::HashMap<Uuid, Uuid> =
        all_objects_for_counts.iter().map(|object| (object.id, object.object_type_id)).collect();
    let mut raw_objects = all_objects
        .into_iter()
        .filter(|object| {
            object_matches_filter(object, &object_query, &property_query, &value_query)
        })
        .collect::<Vec<_>>();
    // Keep the first target in the embedded action controls operational: modeled objects whose
    // properties satisfy a governed action's preconditions are shown before unrelated objects.
    raw_objects.sort_by_key(|object| {
        let eligible = action_types_raw.iter().any(|action| {
            action
                .target_object_type_id
                .map(|type_id| type_id == object.object_type_id)
                .unwrap_or(true)
                && satisfies_action_preconditions(&object.properties, &action.preconditions)
        });
        !eligible
    });
    let raw_links = result.1.unwrap_or_default();
    let all_link_instances = result.4.unwrap_or_default();
    let (relationship_matrix_types, relationship_matrix) =
        relationship_matrix(&types, &raw_links, &all_link_instances);
    let (property_coverage_fields, property_coverage) =
        property_coverage(&types, &all_objects_for_counts);
    let graph_path_ids = shortest_object_path(
        &all_objects_for_counts,
        &all_link_instances,
        query.path_from,
        query.path_to,
    );
    let graph_path = graph_path_ids
        .iter()
        .filter_map(|id| all_objects_for_counts.iter().find(|object| object.id == *id))
        .map(|object| GraphPathNode {
            id: object.id,
            type_name: type_names
                .get(&object.object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown type".to_string()),
            summary: object_summaries
                .get(&object.id)
                .cloned()
                .unwrap_or_else(|| object.id.to_string()),
        })
        .collect::<Vec<_>>();
    let raw_link_instances = all_link_instances
        .into_iter()
        .filter(|link| query.link_type_id.map(|id| link.link_type_id == id).unwrap_or(true))
        .collect::<Vec<_>>();
    let matching_link_count = raw_link_instances.len();
    let links = raw_link_instances
        .iter()
        .map(|link| LinkInstanceView {
            id: link.id,
            link_type_id: link.link_type_id,
            link_type_name: raw_links
                .iter()
                .find(|link_type| link_type.id == link.link_type_id)
                .map(|link_type| link_type.name.clone())
                .unwrap_or_else(|| "Unknown relationship".to_string()),
            source_id: link.source_object_id,
            source_summary: object_summaries
                .get(&link.source_object_id)
                .cloned()
                .unwrap_or_else(|| link.source_object_id.to_string()),
            target_id: link.target_object_id,
            target_summary: object_summaries
                .get(&link.target_object_id)
                .cloned()
                .unwrap_or_else(|| link.target_object_id.to_string()),
            properties: serde_json::to_string_pretty(&link.properties).unwrap_or_default(),
        })
        .collect();
    let matching_count = raw_objects.len();
    let graph_depth = query.depth.clamp(1, 3);
    let mut page = query.page.max(0);
    let last_page =
        if matching_count == 0 { 0 } else { ((matching_count - 1) / ONTOLOGY_PAGE_SIZE) as i64 };
    page = page.min(last_page);
    if let Some(selected_object) = query.object_id {
        if let Some(index) = raw_objects.iter().position(|object| object.id == selected_object) {
            // Deep links from cards, lineage, and the graph omit a page parameter. Put the
            // selected object on screen instead of forcing the operator to hunt through pages.
            if query.page == 0 {
                page = (index / ONTOLOGY_PAGE_SIZE) as i64;
            }
        }
    }
    let start = (page as usize).saturating_mul(ONTOLOGY_PAGE_SIZE);
    let has_prev = page > 0;
    let has_next = start.saturating_add(ONTOLOGY_PAGE_SIZE) < matching_count;
    let visible_object_ids = raw_objects
        .iter()
        .skip(start)
        .take(ONTOLOGY_PAGE_SIZE)
        .map(|object| object.id)
        .collect::<std::collections::HashSet<_>>();
    // Keep the graph bounded for a readable SVG, but always center a deep link on its
    // selected object and its immediate neighborhood. Previously a selected object beyond
    // the first 24 could open in the list while being absent from the graph entirely.
    let neighborhood =
        graph_neighborhood(&raw_objects, &raw_link_instances, query.object_id, graph_depth, 24);
    let mut graph_objects = neighborhood
        .iter()
        .filter_map(|id| raw_objects.iter().find(|object| object.id == *id))
        .collect::<Vec<_>>();
    if query.object_id.is_none() {
        for object in &raw_objects {
            if graph_objects.len() >= 24 {
                break;
            }
            if !graph_objects.iter().any(|item| item.id == object.id) {
                graph_objects.push(object);
            }
        }
    }
    let graph_positions: std::collections::HashMap<Uuid, (i32, i32)> = graph_objects
        .iter()
        .enumerate()
        .map(|(index, object)| {
            (object.id, (130 + ((index % 4) as i32 * 220), 75 + ((index / 4) as i32 * 105)))
        })
        .collect();
    let graph_nodes = graph_objects
        .iter()
        .enumerate()
        .map(|(index, object)| {
            let (x, y) = graph_positions[&object.id];
            GraphNodeView {
                id: object.id,
                type_name: type_names
                    .get(&object.object_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown type".to_string()),
                summary: object_summaries
                    .get(&object.id)
                    .cloned()
                    .unwrap_or_else(|| format!("Object {}", index + 1)),
                x,
                y,
            }
        })
        .collect();
    let graph_edges = raw_link_instances
        .iter()
        .filter_map(|link| {
            let (x1, y1) = graph_positions.get(&link.source_object_id).copied()?;
            let (x2, y2) = graph_positions.get(&link.target_object_id).copied()?;
            Some(GraphEdgeView {
                source_id: link.source_object_id,
                target_id: link.target_object_id,
                x1,
                y1,
                x2,
                y2,
                label: raw_links
                    .iter()
                    .find(|link_type| link_type.id == link.link_type_id)
                    .map(|link_type| link_type.name.clone())
                    .unwrap_or_else(|| "related".to_string()),
            })
        })
        .collect();
    let invocations = result.3.unwrap_or_default();
    let reviews = client.list_action_reviews(token).await.unwrap_or_default();
    let reviews = reviews
        .into_iter()
        .map(|review| (review.invocation_id, review))
        .collect::<std::collections::HashMap<_, _>>();
    // Join modeled entities back to the operational stream through their source lineage. This
    // keeps an ontology card useful during investigation: a sourced object can expose the
    // signal and case context that caused it to matter without requiring a second search.
    let (event_page, incidents_result) = tokio::join!(
        state.events_client.list_events(token, 1000, 0, None, None),
        state.incidents_client.list_incidents(session.tenant_id, None),
    );
    let context_events = event_page.map(|page| page.events).unwrap_or_default();
    let context_incidents = incidents_result.unwrap_or_default();
    let selected_history = match query.object_id {
        Some(object_id) => client.list_object_history(token, object_id).await.unwrap_or_default(),
        None => vec![],
    };
    let action_names: std::collections::HashMap<Uuid, String> =
        action_types.iter().map(|action| (action.id, action.name.clone())).collect();
    let mut objects = Vec::new();
    for o in raw_objects.iter().filter(|object| visible_object_ids.contains(&object.id)) {
        let related = raw_link_instances
            .iter()
            .filter_map(|link| {
                let (related_id, incoming) = if link.source_object_id == o.id {
                    (link.target_object_id, false)
                } else if link.target_object_id == o.id {
                    (link.source_object_id, true)
                } else {
                    return None;
                };
                let relationship = raw_links
                    .iter()
                    .find(|link_type| link_type.id == link.link_type_id)
                    .map(|link_type| link_type.name.clone())
                    .unwrap_or_else(|| "Related object".to_string());
                Some(RelatedObjectView {
                    relationship: if incoming {
                        format!("{relationship} · incoming")
                    } else {
                        relationship
                    },
                    id: related_id,
                    type_name: object_type_ids
                        .get(&related_id)
                        .and_then(|type_id| type_names.get(type_id))
                        .cloned()
                        .unwrap_or_else(|| "Unknown type".to_string()),
                    summary: object_summaries
                        .get(&related_id)
                        .cloned()
                        .unwrap_or_else(|| "View object".to_string()),
                })
            })
            .collect();
        let lineage_ids: Vec<Uuid> = o
            .source_lineage
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .filter_map(|v| Uuid::parse_str(v).ok())
            .collect();
        let (signals, incidents) =
            object_operational_context(&lineage_ids, &context_events, &context_incidents);
        let history = if query.object_id == Some(o.id) {
            selected_history
                .iter()
                .map(|entry| ObjectHistoryView {
                    change_type: entry.change_type.clone(),
                    actor: entry.actor.clone(),
                    before_state: entry
                        .before_state
                        .as_ref()
                        .map(|value| serde_json::to_string_pretty(value).unwrap_or_default())
                        .unwrap_or_else(|| "(none)".to_string()),
                    after_state: entry
                        .after_state
                        .as_ref()
                        .map(|value| serde_json::to_string_pretty(value).unwrap_or_default())
                        .unwrap_or_else(|| "(none)".to_string()),
                    changed_at: entry.changed_at,
                })
                .collect()
        } else {
            vec![]
        };
        let mut property_rows = o
            .properties
            .as_object()
            .into_iter()
            .flat_map(|properties| properties.iter())
            .map(|(key, value)| PropertyView {
                key: key.clone(),
                value: if value.is_string() {
                    value.as_str().unwrap_or_default().to_string()
                } else {
                    serde_json::to_string(value).unwrap_or_default()
                },
            })
            .collect::<Vec<_>>();
        property_rows.sort_by(|left, right| left.key.cmp(&right.key));
        let editor_fields = types
            .iter()
            .find(|object_type| object_type.id == o.object_type_id)
            .map(|object_type| {
                build_property_editor_fields(&object_type.property_schema, &o.properties)
            })
            .unwrap_or_default();
        let mut activity = invocations
            .iter()
            .filter(|invocation| {
                invocation.target_object_ids.as_array().into_iter().flatten().any(|target| {
                    target.as_str().and_then(|value| Uuid::parse_str(value).ok()) == Some(o.id)
                })
            })
            .map(|invocation| ObjectActivityView {
                id: invocation.id,
                action_name: action_names
                    .get(&invocation.action_type_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown governed action".to_string()),
                outcome: invocation.outcome.clone(),
                review_status: reviews
                    .get(&invocation.id)
                    .map(|review| review.status.replace('_', " "))
                    .unwrap_or_else(|| "not reviewed".to_string()),
                review_assignee: reviews
                    .get(&invocation.id)
                    .and_then(|review| review.assignee.clone()),
                review_stale: reviews
                    .get(&invocation.id)
                    .map(|review| {
                        !matches!(review.status.as_str(), "approved" | "declined")
                            && review.due_at.is_some_and(|due_at| due_at <= chrono::Utc::now())
                    })
                    .unwrap_or(false),
                executed_at: invocation.executed_at,
                event_id: invocation
                    .triggering_event_ref
                    .get("event_id")
                    .or_else(|| invocation.triggering_event_ref.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok()),
            })
            .collect::<Vec<_>>();
        activity.sort_by(|left, right| right.executed_at.cmp(&left.executed_at));
        activity.truncate(8);
        let (risk_score, risk_label, risk_tone, risk_reasons) =
            object_risk_posture(&signals, &incidents, &activity);
        objects.push(ObjectView {
            id: o.id,
            object_type_id: o.object_type_id,
            type_name: type_names
                .get(&o.object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown type".to_string()),
            summary: o
                .properties
                .get("name")
                .or_else(|| o.properties.get("subject"))
                .or_else(|| o.properties.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled object")
                .to_string(),
            properties: serde_json::to_string_pretty(&o.properties).unwrap_or_default(),
            property_rows,
            editor_fields,
            lineage: serde_json::to_string_pretty(&o.source_lineage).unwrap_or_default(),
            lineage_ids,
            related,
            signals,
            incidents,
            history,
            activity,
            risk_score,
            risk_label,
            risk_tone,
            risk_reasons,
            updated_at: o.updated_at,
        });
    }
    let risk_metric_order = [
        ("critical", "Critical attention"),
        ("warning", "Needs attention"),
        ("neutral", "Monitored"),
        ("good", "Stable"),
    ];
    let total_risk_objects = objects.len();
    let risk_metrics = risk_metric_order
        .into_iter()
        .map(|(key, label)| {
            let count = objects.iter().filter(|object| object.risk_tone == key).count();
            ObjectRiskMetric {
                key: key.to_string(),
                label: label.to_string(),
                count,
                percent: if total_risk_objects == 0 {
                    0
                } else {
                    (count * 100 / total_risk_objects) as i32
                },
            }
        })
        .collect::<Vec<_>>();
    let risk_filter = match query.risk.trim().to_ascii_lowercase().as_str() {
        "critical" | "warning" | "neutral" | "good" => query.risk.trim().to_ascii_lowercase(),
        _ => String::new(),
    };
    if !risk_filter.is_empty() {
        objects.retain(|object| object.risk_tone == risk_filter);
    }
    let displayed_matching_count =
        if risk_filter.is_empty() { matching_count } else { objects.len() };
    let link_types = raw_links
        .iter()
        .map(|l| LinkTypeView {
            id: l.id,
            name: l.name.clone(),
            cardinality: l.cardinality.clone(),
            source_id: l.source_object_type_id,
            target_id: l.target_object_type_id,
            source_name: type_names
                .get(&l.source_object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string()),
            target_name: type_names
                .get(&l.target_object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string()),
            selected: query.link_type_id == Some(l.id),
            instance_count: raw_link_instances
                .iter()
                .filter(|instance| instance.link_type_id == l.id)
                .count(),
        })
        .collect();
    let action_invocations = invocations
        .iter()
        .map(|a| ActionInvocationView {
            action_name: action_names
                .get(&a.action_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown governed action".to_string()),
            outcome: a.outcome.clone(),
            target_count: a.target_object_ids.as_array().map(|v| v.len()).unwrap_or(0),
            executed_at: a.executed_at,
        })
        .collect();
    Html(
        OntologyTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            can_manage: session.role.at_least(common::Role::Operator),
            object_types,
            link_types,
            links,
            graph_nodes,
            graph_edges,
            graph_total_count: matching_count,
            graph_depth,
            path_from: query.path_from,
            path_to: query.path_to,
            graph_path,
            objects,
            matching_count: displayed_matching_count,
            page,
            has_prev,
            has_next,
            relationship_objects,
            action_invocations,
            action_types,
            selected_type: query.type_id,
            selected_object: query.object_id,
            q: query.q.clone(),
            property: query.property.clone(),
            value: query.value.clone(),
            risk: query.risk.clone(),
            error,
            saved_views,
            notice: query.notice,
            created_count: query.created_count,
            failed_count: query.failed_count,
            link_type_filter: query.link_type_id,
            matching_link_count,
            relationship_matrix_types,
            relationship_matrix,
            property_coverage_fields,
            property_coverage,
            risk_metrics,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct OntologyCompareQuery {
    #[serde(default)]
    pub ids: String,
}

/// GET /ontology/compare — compare a bounded object set selected in the ontology workbench.
pub async fn get_ontology_compare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyCompareQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let ids = query
        .ids
        .split(',')
        .filter_map(|value| Uuid::parse_str(value.trim()).ok())
        .take(6)
        .collect::<Vec<_>>();
    let types = client.list_object_types(&session.bearer_token).await.unwrap_or_default();
    let type_names = types
        .into_iter()
        .map(|item| (item.id, item.name))
        .collect::<std::collections::HashMap<_, _>>();
    let all_objects = client.list_objects(&session.bearer_token, None).await.unwrap_or_default();
    let by_id = all_objects
        .into_iter()
        .map(|object| (object.id, object))
        .collect::<std::collections::HashMap<_, _>>();
    let objects = ids
        .into_iter()
        .filter_map(|id| by_id.get(&id))
        .map(|object| CompareObjectView {
            id: object.id,
            type_name: type_names
                .get(&object.object_type_id)
                .cloned()
                .unwrap_or_else(|| "Unknown type".to_string()),
            summary: object
                .properties
                .get("name")
                .or_else(|| object.properties.get("subject"))
                .or_else(|| object.properties.get("title"))
                .or_else(|| object.properties.get("id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Untitled object")
                .to_string(),
            updated_at: object.updated_at,
            properties: object.properties.as_object().cloned().unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let mut keys = std::collections::BTreeSet::new();
    for object in &objects {
        keys.extend(object.properties.keys().cloned());
    }
    let property_rows: Vec<ComparePropertyRow> = keys
        .into_iter()
        .map(|key| ComparePropertyRow {
            values: objects
                .iter()
                .map(|object| {
                    object.properties.get(&key).map_or_else(
                        || "—".to_string(),
                        |value| {
                            if value.is_string() {
                                value.as_str().unwrap_or_default().to_string()
                            } else {
                                serde_json::to_string(value).unwrap_or_default()
                            }
                        },
                    )
                })
                .collect(),
            key,
        })
        .collect();
    let differing_property_count = property_rows
        .iter()
        .filter(|row| row.values.windows(2).any(|values| values[0] != values[1]))
        .count();
    let shared_property_count = property_rows.len().saturating_sub(differing_property_count);
    Html(
        OntologyCompareTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            objects,
            property_rows,
            differing_property_count,
            shared_property_count,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

/// GET /ontology/export.csv — export the complete active object set, not only the visible page.
pub async fn get_ontology_export_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OntologyQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let objects = match client.list_objects(&session.bearer_token, query.type_id).await {
        Ok(objects) => objects,
        Err(error) => return (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    };
    let text_query = query.q.trim().to_ascii_lowercase();
    let property_query = query.property.trim().to_ascii_lowercase();
    let value_query = query.value.trim().to_ascii_lowercase();
    let type_names = client
        .list_object_types(&session.bearer_token)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|object_type| (object_type.id, object_type.name))
        .collect::<std::collections::HashMap<_, _>>();
    let mut csv = String::from("id,object_type,updated_at,properties,source_lineage\n");
    for object in objects
        .iter()
        .filter(|object| object_matches_filter(object, &text_query, &property_query, &value_query))
    {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            object.id,
            csv_escape(
                type_names
                    .get(&object.object_type_id)
                    .map(String::as_str)
                    .unwrap_or("Unknown type")
            ),
            object.updated_at.to_rfc3339(),
            csv_escape(&object.properties.to_string()),
            csv_escape(&object.source_lineage.to_string()),
        ));
    }
    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(axum::http::header::CONTENT_TYPE, "text/csv".parse().unwrap());
    response_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"ontology-export-{}.csv\"", session.tenant_id)
            .parse()
            .unwrap(),
    );
    (response_headers, csv).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SaveOntologyViewForm {
    pub name: String,
    pub type_id: Option<Uuid>,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub property: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub link_type_id: Option<Uuid>,
}

fn ontology_view_redirect(form: &SaveOntologyViewForm, notice: &str) -> Redirect {
    let mut params = vec![("notice", notice.to_string())];
    if let Some(type_id) = form.type_id {
        params.push(("type_id", type_id.to_string()));
    }
    if !form.q.is_empty() {
        params.push(("q", form.q.clone()));
    }
    if !form.property.is_empty() {
        params.push(("property", form.property.clone()));
    }
    if !form.value.is_empty() {
        params.push(("value", form.value.clone()));
    }
    if !form.risk.is_empty() {
        params.push(("risk", form.risk.clone()));
    }
    if let Some(link_type_id) = form.link_type_id {
        params.push(("link_type_id", link_type_id.to_string()));
    }
    let query = serde_urlencoded::to_string(params).unwrap_or_else(|_| format!("notice={notice}"));
    Redirect::to(&format!("/ontology?{query}"))
}

pub async fn post_save_ontology_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveOntologyViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let name = form.name.trim();
    if name.is_empty() {
        return ontology_view_redirect(&form, "view_invalid").into_response();
    }
    let filter = serde_json::json!({"surface": "ontology", "type_id": form.type_id, "q": form.q, "property": form.property, "value": form.value, "risk": form.risk, "link_type_id": form.link_type_id});
    match state.saved_search_queries_client.create(session.tenant_id, name, filter).await {
        Ok(_) => ontology_view_redirect(&form, "view_saved").into_response(),
        Err(_) => ontology_view_redirect(&form, "view_failed").into_response(),
    }
}

pub async fn post_delete_ontology_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(_) => Redirect::to("/ontology?notice=view_deleted").into_response(),
        Err(_) => Redirect::to("/ontology?notice=view_failed").into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ObjectForm {
    pub object_type_id: Uuid,
    pub properties: String,
    pub source_lineage: String,
}

fn parse_object_form(form: ObjectForm) -> Result<CreateObjectRequest, Response> {
    let properties = serde_json::from_str(&form.properties).map_err(|_| {
        (StatusCode::BAD_REQUEST, "Object properties must be valid JSON").into_response()
    })?;
    let source_lineage = serde_json::from_str(&form.source_lineage).map_err(|_| {
        (StatusCode::BAD_REQUEST, "Source lineage must be valid JSON").into_response()
    })?;
    Ok(CreateObjectRequest { object_type_id: form.object_type_id, properties, source_lineage })
}

pub async fn create_ontology_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ObjectForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let input = match parse_object_form(form) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.create_object(&session.bearer_token, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=object_created").into_response(),
        Err(crate::ontology_client::OntologyClientError::Rejected(400)) => (
            StatusCode::BAD_REQUEST,
            "Object properties do not match the selected object type schema",
        )
            .into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn update_ontology_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ObjectForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let input = match parse_object_form(form) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.update_object(&session.bearer_token, id, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=object_updated").into_response(),
        Err(crate::ontology_client::OntologyClientError::Rejected(400)) => (
            StatusCode::BAD_REQUEST,
            "Object properties do not match the selected object type schema",
        )
            .into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn delete_ontology_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.delete_object(&session.bearer_token, id).await {
        Ok(()) => Redirect::to("/ontology?notice=object_deleted").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct LinkTypeForm {
    pub name: String,
    pub source_object_type_id: Uuid,
    pub target_object_type_id: Uuid,
    pub cardinality: String,
}
#[derive(Debug, Deserialize)]
pub struct LinkInstanceForm {
    pub link_type_id: Uuid,
    pub source_object_id: Uuid,
    pub target_object_id: Uuid,
    pub properties: String,
}
#[derive(Debug, Deserialize)]
pub struct BulkLinkInstanceForm {
    pub link_type_id: Uuid,
    pub target_object_id: Uuid,
    pub source_object_ids: String,
    pub properties: String,
}
#[derive(Debug, Deserialize)]
pub struct ActionTypeForm {
    pub name: String,
    pub target_object_type_id: Option<Uuid>,
    pub parameter_schema: String,
    pub preconditions: String,
    pub effect_definition: String,
}

fn parse_link_properties(value: String) -> Result<Option<serde_json::Value>, Response> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&value).map(Some).map_err(|_| {
        (StatusCode::BAD_REQUEST, "Link properties must be valid JSON").into_response()
    })
}

pub async fn create_ontology_link_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LinkInstanceForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let properties = match parse_link_properties(form.properties) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let input = CreateLinkRequest {
        link_type_id: form.link_type_id,
        source_object_id: form.source_object_id,
        target_object_id: form.target_object_id,
        properties,
    };
    match client.create_link(&session.bearer_token, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=relationship_created").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

/// POST /ontology/links/instances/bulk — creates up to 25 instances from the selected source
/// objects to one governed target. The ontology service remains the final type/cardinality
/// authority; this UI workflow reports partial success rather than hiding rejected pairs.
pub async fn create_bulk_ontology_link_instances(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<BulkLinkInstanceForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let properties = match parse_link_properties(form.properties) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return Redirect::to("/ontology?notice=bulk_relationship_failed").into_response();
    };
    let source_ids = form
        .source_object_ids
        .split(',')
        .filter_map(|value| Uuid::parse_str(value.trim()).ok())
        .take(25)
        .collect::<Vec<_>>();
    if source_ids.is_empty() {
        return Redirect::to("/ontology?notice=bulk_relationship_empty").into_response();
    }
    let mut created_count = 0usize;
    let mut failed_count = 0usize;
    for source_object_id in source_ids {
        let input = CreateLinkRequest {
            link_type_id: form.link_type_id,
            source_object_id,
            target_object_id: form.target_object_id,
            properties: properties.clone(),
        };
        match client.create_link(&session.bearer_token, &input).await {
            Ok(()) => created_count += 1,
            Err(_) => failed_count += 1,
        }
    }
    Redirect::to(&format!(
        "/ontology?notice=bulk_relationship_created&created_count={created_count}&failed_count={failed_count}"
    ))
    .into_response()
}

pub async fn update_ontology_link_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<LinkInstanceForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let properties = match parse_link_properties(form.properties) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let input = CreateLinkRequest {
        link_type_id: form.link_type_id,
        source_object_id: form.source_object_id,
        target_object_id: form.target_object_id,
        properties,
    };
    match client.update_link(&session.bearer_token, id, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=relationship_updated").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

pub async fn delete_ontology_link_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.delete_link(&session.bearer_token, id).await {
        Ok(()) => Redirect::to("/ontology?notice=relationship_deleted").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ActionInvocationForm {
    pub action_type_id: Uuid,
    #[serde(default)]
    pub target_object_id: Option<Uuid>,
    #[serde(default)]
    pub target_object_ids: Option<String>,
    pub parameters: String,
    pub return_to: Option<String>,
    pub event_id: Option<Uuid>,
    pub incident_id: Option<Uuid>,
}

fn action_redirect(return_to: Option<&str>, notice: &str) -> Redirect {
    match return_to {
        Some(path) if path.starts_with("/actions?") => {
            Redirect::to(&format!("{path}&notice={notice}"))
        }
        Some(path) if path.starts_with("/actions/library?") => {
            Redirect::to(&format!("{path}&notice={notice}"))
        }
        Some("/actions/library") => Redirect::to(&format!("/actions/library?notice={notice}")),
        Some("actions") => Redirect::to(&format!("/actions?notice={notice}")),
        Some("/work") => Redirect::to(&format!("/work?notice={notice}")),
        Some(path)
            if path.starts_with("/events/")
                && Uuid::parse_str(path.trim_start_matches("/events/")).is_ok() =>
        {
            Redirect::to(&format!("{path}?notice={notice}"))
        }
        Some(path)
            if path.starts_with("/incidents/")
                && Uuid::parse_str(path.trim_start_matches("/incidents/")).is_ok() =>
        {
            Redirect::to(&format!("{path}?notice={notice}"))
        }
        _ => Redirect::to(&format!("/ontology?notice={notice}")),
    }
}

pub async fn invoke_ontology_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ActionInvocationForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let parameters = match serde_json::from_str(&form.parameters) {
        Ok(value) => value,
        Err(_) => {
            return action_redirect(form.return_to.as_deref(), "invalid-parameters").into_response()
        }
    };
    let Some(client) = ontology_client::global() else {
        return action_redirect(form.return_to.as_deref(), "rejected").into_response();
    };
    let action_types = match client.list_action_types(&session.bearer_token).await {
        Ok(types) => types,
        Err(_) => return action_redirect(form.return_to.as_deref(), "rejected").into_response(),
    };
    let parameters = match action_types.iter().find(|action| action.id == form.action_type_id) {
        Some(action) => default_action_parameters(&action.parameter_schema, parameters),
        None => return action_redirect(form.return_to.as_deref(), "rejected").into_response(),
    };
    let mut target_object_ids = form
        .target_object_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|value| Uuid::parse_str(value.trim()).ok())
        .collect::<Vec<_>>();
    if target_object_ids.is_empty() {
        if let Some(target_object_id) = form.target_object_id {
            target_object_ids.push(target_object_id);
        }
    }
    if target_object_ids.is_empty() {
        return action_redirect(form.return_to.as_deref(), "rejected").into_response();
    }
    let triggering_event_ref = match (form.event_id, form.incident_id) {
        (Some(event_id), Some(incident_id)) => Some(
            serde_json::json!({"event_id": event_id, "incident_id": incident_id, "source": "console", "actor": session.username}),
        ),
        (Some(event_id), None) => Some(
            serde_json::json!({"event_id": event_id, "source": "console", "actor": session.username}),
        ),
        (None, Some(incident_id)) => Some(
            serde_json::json!({"incident_id": incident_id, "source": "console", "actor": session.username}),
        ),
        (None, None) => None,
    };
    let input = InvokeActionRequest {
        action_type_id: form.action_type_id,
        target_object_ids,
        parameters,
        triggering_event_ref,
    };
    match client.invoke_action(&session.bearer_token, &input).await {
        Ok(_) => action_redirect(form.return_to.as_deref(), "executed").into_response(),
        Err(ontology_client::OntologyClientError::Rejected(_)) => {
            action_redirect(form.return_to.as_deref(), "rejected").into_response()
        }
        Err(_) => action_redirect(form.return_to.as_deref(), "rejected").into_response(),
    }
}

pub async fn create_ontology_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LinkTypeForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let input = CreateLinkTypeRequest {
        name: form.name,
        source_object_type_id: form.source_object_type_id,
        target_object_type_id: form.target_object_type_id,
        cardinality: form.cardinality,
        properties_schema: None,
    };
    match client.create_link_type(&session.bearer_token, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=relationship_type_created").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn delete_ontology_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.delete_link_type(&session.bearer_token, id).await {
        Ok(()) => Redirect::to("/ontology?notice=relationship_type_deleted").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

pub async fn update_ontology_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<LinkTypeForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let input = CreateLinkTypeRequest {
        name: form.name,
        source_object_type_id: form.source_object_type_id,
        target_object_type_id: form.target_object_type_id,
        cardinality: form.cardinality,
        properties_schema: None,
    };
    match client.update_link_type(&session.bearer_token, id, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=relationship_type_updated").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn create_ontology_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ActionTypeForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let parse = |v: String| serde_json::from_str(&v).unwrap_or(serde_json::json!({}));
    let input = CreateActionTypeRequest {
        name: form.name,
        target_object_type_id: form.target_object_type_id,
        parameter_schema: parse(form.parameter_schema),
        preconditions: parse(form.preconditions),
        effect_definition: parse(form.effect_definition),
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.create_action_type(&session.bearer_token, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=action_created").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn delete_ontology_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.delete_action_type(&session.bearer_token, id).await {
        Ok(()) => Redirect::to("/ontology?notice=action_deleted").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

pub async fn update_ontology_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionTypeForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let parse = |v: String| serde_json::from_str(&v).unwrap_or(serde_json::json!({}));
    let input = CreateActionTypeRequest {
        name: form.name,
        target_object_type_id: form.target_object_type_id,
        parameter_schema: parse(form.parameter_schema),
        preconditions: parse(form.preconditions),
        effect_definition: parse(form.effect_definition),
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.update_action_type(&session.bearer_token, id, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=action_updated").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn create_ontology_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ObjectTypeForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let schema = match serde_json::from_str(&form.property_schema) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Property schema must be valid JSON").into_response()
        }
    };
    let rules = match serde_json::from_str(&form.mapping_rules) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Mapping rules must be valid JSON").into_response()
        }
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client
        .create_object_type(
            &session.bearer_token,
            &CreateObjectTypeRequest {
                name: form.name,
                version: form.version,
                property_schema: schema,
                mapping_rules: rules,
            },
        )
        .await
    {
        Ok(()) => Redirect::to("/ontology?notice=type_created").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn update_ontology_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ObjectTypeForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Operator) {
        return (StatusCode::FORBIDDEN, "Operator access required").into_response();
    }
    let schema = match serde_json::from_str(&form.property_schema) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Property schema must be valid JSON").into_response()
        }
    };
    let rules = match serde_json::from_str(&form.mapping_rules) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Mapping rules must be valid JSON").into_response()
        }
    };
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    let input = CreateObjectTypeRequest {
        name: form.name,
        version: form.version,
        property_schema: schema,
        mapping_rules: rules,
    };
    match client.update_object_type(&session.bearer_token, id, &input).await {
        Ok(()) => Redirect::to("/ontology?notice=type_updated").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn delete_ontology_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    if !session.role.at_least(common::Role::Admin) {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }
    let Some(client) = ontology_client::global() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Ontology client unavailable").into_response();
    };
    match client.delete_object_type(&session.bearer_token, id).await {
        Ok(()) => Redirect::to("/ontology?notice=type_deleted").into_response(),
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}
