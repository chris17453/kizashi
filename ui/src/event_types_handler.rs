#[path = "event_types_handler_test.rs"]
#[cfg(test)]
mod event_types_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, EventSummary, TriggerSummary};
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use common::{EventTypeDefinition, Role};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, serde::Deserialize, Default)]
pub struct EventTypesQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub coverage: String,
}

#[derive(Clone)]
struct EventTypeView {
    name: String,
    count: usize,
    last_seen: DateTime<Utc>,
    fields: Vec<String>,
    triggers: Vec<String>,
    sample_event_id: uuid::Uuid,
    has_sample: bool,
    registered: bool,
    definition_id: uuid::Uuid,
    version: i32,
    schema_pretty: String,
    source_mapping_pretty: String,
    versions: Vec<VersionView>,
}

#[derive(Clone)]
struct VersionView {
    id: uuid::Uuid,
    version: i32,
    schema_pretty: String,
    mapping_pretty: String,
    changes: Vec<String>,
}

#[derive(Template)]
#[template(path = "event_types.html")]
struct EventTypesTemplate {
    show_nav: bool,
    is_admin: bool,
    tenant_id: uuid::Uuid,
    username: String,
    event_types: Vec<EventTypeView>,
    total_events: usize,
    error: Option<String>,
    q: String,
    notice: String,
    can_write: bool,
    governed_count: usize,
    triggerless_count: usize,
    observed_only_count: usize,
    activity_bars: Vec<EventActivityBar>,
    coverage_scope: String,
}

fn normalize_event_coverage(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "governed" | "observed_only" | "triggerless" => value.trim().to_ascii_lowercase(),
        _ => String::new(),
    }
}

struct EventActivityBar {
    date: String,
    count: usize,
    height_pct: i32,
}

fn event_activity_bars(events: &[EventSummary]) -> Vec<EventActivityBar> {
    let mut counts = BTreeMap::<String, usize>::new();
    for event in events {
        *counts.entry(event.occurred_at.date_naive().to_string()).or_default() += 1;
    }
    let max = counts.values().copied().max().unwrap_or(1);
    counts
        .into_iter()
        .map(|(date, count)| EventActivityBar {
            date,
            count,
            height_pct: ((count * 100) / max).max(8) as i32,
        })
        .collect()
}

fn payload_fields(value: &serde_json::Value, prefix: &str, fields: &mut BTreeSet<String>) {
    if let serde_json::Value::Object(object) = value {
        for (key, child) in object {
            let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
            fields.insert(format!("{path}: {}", json_type(child)));
            payload_fields(child, &path, fields);
        }
    }
}

fn json_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

const SOURCE_MAPPING_KEY: &str = "x-kizashi-source-mapping";

fn source_mapping_pretty(schema: &serde_json::Value) -> String {
    schema
        .get(SOURCE_MAPPING_KEY)
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_else(|| "{}".to_string())
}

fn apply_source_mapping(
    mut schema: serde_json::Value,
    raw_mapping: &str,
) -> Result<serde_json::Value, &'static str> {
    if raw_mapping.trim().is_empty() {
        return Ok(schema);
    }
    let mapping: serde_json::Value = serde_json::from_str(raw_mapping.trim())
        .map_err(|_| "source mapping must be valid JSON")?;
    if !mapping.is_object() {
        return Err("source mapping must be a JSON object of event fields to source paths");
    }
    let object = schema.as_object_mut().ok_or("field schema must be a JSON object")?;
    object.insert(SOURCE_MAPPING_KEY.to_string(), mapping);
    Ok(schema)
}

fn contract_changes(
    previous: Option<&EventTypeDefinition>,
    current: &EventTypeDefinition,
) -> Vec<String> {
    let Some(previous) = previous else {
        return vec!["Initial contract publication".to_string()];
    };
    let previous_properties = previous
        .field_schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let current_properties = current
        .field_schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut changes = Vec::new();
    for key in current_properties.keys() {
        if !previous_properties.contains_key(key) {
            changes.push(format!("Added field `{key}`"));
        } else if previous_properties.get(key) != current_properties.get(key) {
            changes.push(format!("Changed field `{key}`"));
        }
    }
    for key in previous_properties.keys() {
        if !current_properties.contains_key(key) {
            changes.push(format!("Removed field `{key}`"));
        }
    }
    if source_mapping_pretty(&previous.field_schema) != source_mapping_pretty(&current.field_schema)
    {
        changes.push("Changed source mapping".to_string());
    }
    if changes.is_empty() {
        changes.push("No field or source mapping changes".to_string());
    }
    changes
}

fn build_views(
    events: Vec<EventSummary>,
    samples: BTreeMap<String, (uuid::Uuid, Vec<String>)>,
    triggers: Vec<TriggerSummary>,
    definitions: Vec<EventTypeDefinition>,
    q: &str,
) -> Vec<EventTypeView> {
    let mut grouped: BTreeMap<String, (usize, DateTime<Utc>)> = BTreeMap::new();
    for event in &events {
        let entry = grouped.entry(event.event_type.clone()).or_insert((0, event.occurred_at));
        entry.0 += 1;
        entry.1 = entry.1.max(event.occurred_at);
    }
    let query = q.trim().to_lowercase();
    let mut definitions_by_name: BTreeMap<String, Vec<EventTypeDefinition>> = BTreeMap::new();
    for definition in definitions {
        definitions_by_name.entry(definition.name.clone()).or_default().push(definition);
    }
    let mut names: BTreeSet<String> = grouped.keys().cloned().collect();
    names.extend(definitions_by_name.keys().cloned());
    names
        .into_iter()
        .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
        .map(|name| {
            let (count, last_seen) = grouped.get(&name).cloned().unwrap_or((0, Utc::now()));
            let (sample_event_id, fields) =
                samples.get(&name).cloned().unwrap_or((uuid::Uuid::nil(), vec![]));
            let consuming = triggers
                .iter()
                .filter(|trigger| trigger.event_type_match == name)
                .map(|trigger| trigger.name.clone())
                .collect();
            let mut versions = definitions_by_name.get(&name).cloned().unwrap_or_default();
            versions.sort_by_key(|definition| std::cmp::Reverse(definition.version));
            let latest = versions.first().cloned();
            let mut ascending_versions = versions.clone();
            ascending_versions.sort_by_key(|definition| definition.version);
            let version_views = versions
                .iter()
                .map(|definition| {
                    let previous = ascending_versions
                        .iter()
                        .position(|candidate| candidate.id == definition.id)
                        .and_then(|index| index.checked_sub(1))
                        .and_then(|index| ascending_versions.get(index));
                    VersionView {
                        id: definition.id,
                        version: definition.version,
                        schema_pretty: serde_json::to_string_pretty(&definition.field_schema)
                            .unwrap_or_else(|_| "{}".to_string()),
                        mapping_pretty: source_mapping_pretty(&definition.field_schema),
                        changes: contract_changes(previous, definition),
                    }
                })
                .collect();
            EventTypeView {
                name,
                count,
                last_seen,
                fields,
                triggers: consuming,
                has_sample: sample_event_id != uuid::Uuid::nil(),
                sample_event_id,
                registered: latest.is_some(),
                definition_id: latest
                    .as_ref()
                    .map(|definition| definition.id)
                    .unwrap_or(uuid::Uuid::nil()),
                version: latest.as_ref().map(|definition| definition.version).unwrap_or(0),
                source_mapping_pretty: latest
                    .as_ref()
                    .map(|definition| source_mapping_pretty(&definition.field_schema))
                    .unwrap_or_else(|| "{}".into()),
                schema_pretty: latest
                    .map(|definition| {
                        serde_json::to_string_pretty(&definition.field_schema)
                            .unwrap_or_else(|_| "{}".into())
                    })
                    .unwrap_or_default(),
                versions: version_views,
            }
        })
        .collect()
}

pub async fn get_event_types(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventTypesQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let (events_result, triggers_result, definitions_result) = tokio::join!(
        state.events_client.list_events(&session.bearer_token, 1000, 0, None, None),
        state.triggers_client.list_triggers(session.tenant_id, 1000, 0),
        state.saved_search_queries_client.list_event_types(session.tenant_id, true),
    );
    let events = match events_result {
        Ok(page) => page.events,
        Err(error) => {
            return Html(
                EventTypesTemplate {
                    show_nav: true,
                    is_admin: session.role.at_least(Role::Admin),
                    tenant_id: session.tenant_id,
                    username: session.username.clone(),
                    event_types: vec![],
                    total_events: 0,
                    error: Some(error.to_string()),
                    q: query.q,
                    notice: String::new(),
                    can_write: session.role.at_least(Role::Operator),
                    governed_count: 0,
                    triggerless_count: 0,
                    observed_only_count: 0,
                    activity_bars: vec![],
                    coverage_scope: normalize_event_coverage(&query.coverage),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };
    let triggers = triggers_result.map(|page| page.triggers).unwrap_or_default();
    let definitions = definitions_result.unwrap_or_default();
    let mut sample_ids = BTreeMap::new();
    for event in &events {
        sample_ids.entry(event.event_type.clone()).or_insert(event.id);
    }
    let mut samples = BTreeMap::new();
    for (name, id) in sample_ids.into_iter().take(50) {
        let fields = match state.events_client.get_event(&session.bearer_token, id).await {
            Ok(Some(detail)) => {
                let mut set = BTreeSet::new();
                payload_fields(&detail.payload, "", &mut set);
                set.into_iter().collect()
            }
            _ => vec![],
        };
        samples.insert(name, (id, fields));
    }
    let total_events = events.len();
    let activity_bars = event_activity_bars(&events);
    let mut event_types = build_views(events, samples, triggers, definitions, &query.q);
    let coverage_scope = normalize_event_coverage(&query.coverage);
    if !coverage_scope.is_empty() {
        event_types.retain(|item| match coverage_scope.as_str() {
            "governed" => item.registered,
            "observed_only" => !item.registered,
            "triggerless" => item.triggers.is_empty(),
            _ => true,
        });
    }
    let governed_count = event_types.iter().filter(|item| item.registered).count();
    let triggerless_count = event_types.iter().filter(|item| item.triggers.is_empty()).count();
    let observed_only_count = event_types.iter().filter(|item| !item.registered).count();
    Html(
        EventTypesTemplate {
            show_nav: true,
            is_admin: session.role.at_least(Role::Admin),
            tenant_id: session.tenant_id,
            username: session.username,
            event_types,
            total_events,
            error: None,
            q: query.q,
            notice: query.notice,
            can_write: session.role.at_least(Role::Operator),
            governed_count,
            triggerless_count,
            observed_only_count,
            activity_bars,
            coverage_scope,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct EventTypeCreateForm {
    name: String,
    field_schema: String,
    #[serde(default)]
    source_mapping: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    coverage: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct EventTypeVersionForm {
    field_schema: String,
    #[serde(default)]
    source_mapping: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    coverage: String,
}

fn event_type_redirect(notice: &str, q: &str, coverage: &str) -> Redirect {
    let mut params = vec![("notice", notice.to_string())];
    if !q.is_empty() {
        params.push(("q", q.to_string()));
    }
    let coverage = normalize_event_coverage(coverage);
    if !coverage.is_empty() {
        params.push(("coverage", coverage));
    }
    Redirect::to(&format!(
        "/event-types?{}",
        serde_urlencoded::to_string(params).unwrap_or_default()
    ))
}

pub async fn post_create_event_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<EventTypeCreateForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return event_type_redirect("forbidden", &form.q, &form.coverage).into_response();
    }
    let schema = match serde_json::from_str(&form.field_schema)
        .ok()
        .and_then(|schema| apply_source_mapping(schema, &form.source_mapping).ok())
    {
        Some(schema) => schema,
        None => {
            return event_type_redirect("invalid-schema", &form.q, &form.coverage).into_response()
        }
    };
    let definition = EventTypeDefinition::new(session.tenant_id, form.name.trim(), schema);
    match state
        .saved_search_queries_client
        .create_event_type(session.role, &session.username, definition)
        .await
    {
        Ok(_) => event_type_redirect("created", &form.q, &form.coverage).into_response(),
        Err(_) => event_type_redirect("create-failed", &form.q, &form.coverage).into_response(),
    }
}

pub async fn post_event_type_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<uuid::Uuid>,
    Form(form): Form<EventTypeVersionForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(Role::Operator) {
        return event_type_redirect("forbidden", &form.q, &form.coverage).into_response();
    }
    let schema = match serde_json::from_str(&form.field_schema)
        .ok()
        .and_then(|schema| apply_source_mapping(schema, &form.source_mapping).ok())
    {
        Some(schema) => schema,
        None => {
            return event_type_redirect("invalid-schema", &form.q, &form.coverage).into_response()
        }
    };
    match state
        .saved_search_queries_client
        .create_event_type_version(session.role, &session.username, session.tenant_id, id, schema)
        .await
    {
        Ok(_) => event_type_redirect("versioned", &form.q, &form.coverage).into_response(),
        Err(_) => event_type_redirect("version-failed", &form.q, &form.coverage).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn groups_types_and_infers_payload_fields() {
        let id = Uuid::new_v4();
        let mut fields = BTreeSet::new();
        payload_fields(
            &serde_json::json!({"account": {"tier": "gold"}, "score": 9}),
            "",
            &mut fields,
        );
        let views = build_views(
            vec![EventSummary {
                id,
                event_type: "risk.alert".into(),
                group_key: "a".into(),
                status: "new".into(),
                occurred_at: Utc::now(),
                record_ids: vec![],
            }],
            BTreeMap::from([("risk.alert".into(), (id, fields.into_iter().collect()))]),
            vec![],
            vec![],
            "",
        );
        assert_eq!(views[0].count, 1);
        assert!(views[0].fields.iter().any(|field| field == "account.tier: string"));
    }

    #[test]
    fn source_mapping_is_persisted_as_a_governed_schema_extension() {
        let schema = apply_source_mapping(
            serde_json::json!({"type": "object"}),
            r#"{"score":"$.analysis.score","entity_ref":"$.customer.id"}"#,
        )
        .unwrap();
        assert_eq!(schema[SOURCE_MAPPING_KEY]["score"], "$.analysis.score");
        assert!(source_mapping_pretty(&schema).contains("customer.id"));
    }

    #[test]
    fn contract_changes_describes_added_changed_removed_and_mapping_fields() {
        let tenant = Uuid::new_v4();
        let previous = EventTypeDefinition::new(
            tenant,
            "risk.alert",
            serde_json::json!({"type":"object","properties":{"score":{"type":"number"},"old":{"type":"string"}},"x-kizashi-source-mapping":{"score":"$.score"}}),
        );
        let current = previous.next_version(serde_json::json!({"type":"object","properties":{"score":{"type":"integer"},"new":{"type":"boolean"}},"x-kizashi-source-mapping":{"score":"$.analysis.score"}}));
        let changes = contract_changes(Some(&previous), &current);
        assert!(changes.iter().any(|change| change.contains("Changed field `score`")));
        assert!(changes.iter().any(|change| change.contains("Added field `new`")));
        assert!(changes.iter().any(|change| change.contains("Removed field `old`")));
        assert!(changes.iter().any(|change| change == "Changed source mapping"));
    }

    #[test]
    fn event_type_redirect_preserves_contract_search_scope() {
        let response =
            event_type_redirect("versioned", "risk.alert", "triggerless").into_response();
        assert_eq!(
            response.headers().get("location").unwrap(),
            "/event-types?notice=versioned&q=risk.alert&coverage=triggerless"
        );
    }
}
