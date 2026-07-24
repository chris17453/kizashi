use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

#[derive(Debug, serde::Deserialize, Default)]
pub struct DataCompareQuery {
    #[serde(default)]
    pub ids: String,
}

pub struct CompareRecord {
    pub id: Uuid,
    pub preview: String,
    pub connector_id: String,
    pub source_type: String,
    pub ingested_at: String,
    pub normalized: bool,
    pub raw_payload: String,
    pub normalized_payload: Option<String>,
    pub raw_field_count: usize,
    pub normalized_field_count: usize,
    pub field_presence: Vec<CompareFieldPresence>,
    pub matrix_fields: Vec<String>,
    pub matrix_values: std::collections::HashMap<String, String>,
    pub signals: Vec<CompareSignal>,
    pub modeled_objects: Vec<CompareObject>,
}

pub struct CompareSignal {
    pub id: Uuid,
    pub event_type: String,
    pub status: String,
}

pub struct CompareObject {
    pub id: Uuid,
    pub type_name: String,
    pub label: String,
}

pub struct CompareFieldPresence {
    pub name: String,
    pub raw: bool,
    pub normalized: bool,
    pub state: String,
}

pub struct CompareMatrixRow {
    pub name: String,
    pub cells: Vec<bool>,
    pub present_count: usize,
    pub disposition: String,
    pub distinct_value_count: usize,
}

#[derive(Template)]
#[template(path = "data_compare.html")]
struct DataCompareTemplate {
    show_nav: bool,
    is_admin: bool,
    records: Vec<CompareRecord>,
    comparison_fields: Vec<CompareMatrixRow>,
    shared_field_count: usize,
    comparison_field_count: usize,
    variable_field_count: usize,
    requested_count: usize,
    error: Option<String>,
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
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

/// GET /data/compare?ids=id1,id2 — bounded, tenant-scoped side-by-side evidence review.
pub async fn get_data_compare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DataCompareQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let ids = query
        .ids
        .split(',')
        .filter_map(|value| Uuid::parse_str(value.trim()).ok())
        .take(4)
        .collect::<Vec<_>>();
    let requested_count = ids.len();
    let mut records = Vec::new();
    let mut errors = Vec::new();
    let (ontology_type_names, ontology_objects) =
        if let Some(client) = crate::ontology_client::global() {
            let (types, objects) = tokio::join!(
                client.list_object_types(&session.bearer_token),
                client.list_objects(&session.bearer_token, None),
            );
            (
                types
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| (item.id, item.name))
                    .collect::<std::collections::HashMap<_, _>>(),
                objects.unwrap_or_default(),
            )
        } else {
            (std::collections::HashMap::new(), vec![])
        };
    for id in ids {
        match state.stats_client.get_record(session.tenant_id, id).await {
            Ok(Some(record)) => {
                let mut compare = to_compare_record(record);
                compare.signals = state
                    .events_client
                    .list_events_for_record(&session.bearer_token, id)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|event| CompareSignal {
                        id: event.id,
                        event_type: event.event_type,
                        status: event.status,
                    })
                    .collect();
                let record_id = id.to_string();
                compare.modeled_objects = ontology_objects
                    .iter()
                    .filter(|object| {
                        object
                            .source_lineage
                            .as_array()
                            .map(|lineage| {
                                lineage
                                    .iter()
                                    .any(|value| value.as_str() == Some(record_id.as_str()))
                            })
                            .unwrap_or(false)
                    })
                    .map(|object| CompareObject {
                        id: object.id,
                        type_name: ontology_type_names
                            .get(&object.object_type_id)
                            .cloned()
                            .unwrap_or_else(|| "Modeled object".to_string()),
                        label: object_label(&object.properties, &object.id.to_string()),
                    })
                    .collect();
                records.push(compare);
            }
            Ok(None) => errors.push(format!("Record {id} was not found in this workspace.")),
            Err(error) => errors.push(error.to_string()),
        }
    }
    let error = if errors.is_empty() { None } else { Some(errors.join(" ")) };
    let mut field_names = std::collections::BTreeSet::new();
    for record in &records {
        field_names.extend(record.matrix_fields.iter().cloned());
    }
    let comparison_fields = field_names
        .into_iter()
        .map(|name| {
            let cells = records
                .iter()
                .map(|record| record.matrix_fields.contains(&name))
                .collect::<Vec<_>>();
            let present_count = cells.iter().filter(|present| **present).count();
            let distinct_value_count = records
                .iter()
                .filter_map(|record| record.matrix_values.get(&name))
                .collect::<std::collections::HashSet<_>>()
                .len();
            let disposition = if present_count == records.len() {
                "shared"
            } else if present_count == 1 {
                "unique"
            } else {
                "partial"
            };
            CompareMatrixRow {
                name,
                cells,
                present_count,
                disposition: disposition.to_string(),
                distinct_value_count,
            }
        })
        .collect::<Vec<_>>();
    let shared_field_count =
        comparison_fields.iter().filter(|field| field.disposition == "shared").count();
    let comparison_field_count = comparison_fields.len();
    let variable_field_count =
        comparison_fields.iter().filter(|field| field.distinct_value_count > 1).count();

    Html(
        DataCompareTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            records,
            comparison_fields,
            shared_field_count,
            comparison_field_count,
            variable_field_count,
            requested_count,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

fn to_compare_record(record: RecordSummary) -> CompareRecord {
    let normalized = record.is_normalized();
    let preview = record.preview();
    let raw_field_count = record.raw_payload.as_object().map(|object| object.len()).unwrap_or(0);
    let normalized_field_count = record
        .normalized_payload
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(|object| object.len())
        .unwrap_or(0);
    let raw_fields = record.raw_payload.as_object().cloned().unwrap_or_default();
    let normalized_fields = record
        .normalized_payload
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut field_names = raw_fields.keys().cloned().collect::<std::collections::BTreeSet<_>>();
    field_names.extend(normalized_fields.keys().cloned());
    let field_presence = field_names
        .into_iter()
        .map(|name| {
            let raw = raw_fields.contains_key(&name);
            let normalized = normalized_fields.contains_key(&name);
            let state = match (raw, normalized) {
                (true, true) => "preserved",
                (true, false) => "raw-only",
                (false, true) => "normalized-only",
                (false, false) => "unknown",
            };
            CompareFieldPresence { name, raw, normalized, state: state.to_string() }
        })
        .collect();
    let mut matrix_fields = raw_fields.keys().cloned().collect::<Vec<_>>();
    matrix_fields
        .extend(normalized_fields.keys().filter(|name| !raw_fields.contains_key(*name)).cloned());
    matrix_fields.sort();
    let matrix_values = raw_fields
        .iter()
        .chain(normalized_fields.iter())
        .map(|(name, value)| {
            (name.clone(), serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
        })
        .collect();
    CompareRecord {
        id: record.id,
        preview,
        connector_id: record.connector_id,
        source_type: record.source_type,
        ingested_at: record.ingested_at.to_rfc3339(),
        normalized,
        raw_payload: pretty(&record.raw_payload),
        normalized_payload: record.normalized_payload.as_ref().map(pretty),
        raw_field_count,
        normalized_field_count,
        field_presence,
        matrix_fields,
        matrix_values,
        signals: vec![],
        modeled_objects: vec![],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn comparison_surface_exposes_value_variance() {
        let template = include_str!("../templates/data_compare.html");
        assert!(template.contains("fields with different values"));
        assert!(template.contains("matrix-variance"));
        assert!(template.contains("variant"));
    }
}
