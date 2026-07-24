#[path = "data_detail_handler_test.rs"]
#[cfg(test)]
mod data_detail_handler_test;

use crate::session::Session;
use crate::session_guard::require_session;
use crate::{AppState, CreateObjectRequest, RecordSummary};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "data_detail.html")]
struct DataDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    notice: String,
    object_types: Vec<ObjectTypeOption>,
    record: Option<RecordSummary>,
    raw_payload_pretty: String,
    normalized_payload_pretty: Option<String>,
    downstream_events: Vec<DownstreamEvent>,
    modeled_objects: Vec<ModeledObjectContext>,
    error: Option<String>,
    raw_field_count: usize,
    normalized_field_count: usize,
    downstream_event_count: usize,
    modeled_object_count: usize,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct DataDetailQuery {
    #[serde(default)]
    pub notice: String,
}

struct DownstreamEvent {
    id: Uuid,
    event_type: String,
    status: String,
    occurred_at: chrono::DateTime<chrono::Utc>,
}
struct ModeledObjectContext {
    id: Uuid,
    type_name: String,
    label: String,
}
struct ObjectTypeOption {
    id: Uuid,
    name: String,
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

async fn downstream_context(
    state: &AppState,
    session: &Session,
    record_id: Uuid,
) -> (Vec<DownstreamEvent>, Vec<ModeledObjectContext>) {
    let downstream_events = state
        .events_client
        .list_events_for_record(&session.bearer_token, record_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|event| DownstreamEvent {
            id: event.id,
            event_type: event.event_type,
            status: event.status,
            occurred_at: event.occurred_at,
        })
        .collect();
    let modeled_objects = if let Some(client) = crate::ontology_client::global() {
        let (types, objects) = tokio::join!(
            client.list_object_types(&session.bearer_token),
            client.list_objects(&session.bearer_token, None),
        );
        let type_names = types
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.id, item.name))
            .collect::<std::collections::HashMap<_, _>>();
        let record_id = record_id.to_string();
        objects
            .unwrap_or_default()
            .into_iter()
            .filter(|object| {
                object
                    .source_lineage
                    .as_array()
                    .map(|lineage| {
                        lineage.iter().any(|item| item.as_str() == Some(record_id.as_str()))
                    })
                    .unwrap_or(false)
            })
            .map(|object| ModeledObjectContext {
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
                    .or_else(|| object.properties.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled object")
                    .to_string(),
            })
            .collect()
    } else {
        vec![]
    };
    (downstream_events, modeled_objects)
}

/// GET /data/:id — the Data Viewer's record detail: full raw and normalized payload.
pub async fn get_data_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<DataDetailQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    match state.stats_client.get_record(session.tenant_id, id).await {
        Ok(Some(record)) => {
            let raw_payload_pretty = pretty(&record.raw_payload);
            let normalized_payload_pretty = record.normalized_payload.as_ref().map(pretty);
            let (downstream_events, modeled_objects) =
                downstream_context(&state, &session, id).await;
            let raw_field_count =
                record.raw_payload.as_object().map(|object| object.len()).unwrap_or(0);
            let normalized_field_count = record
                .normalized_payload
                .as_ref()
                .and_then(serde_json::Value::as_object)
                .map(|object| object.len())
                .unwrap_or(0);
            let downstream_event_count = downstream_events.len();
            let modeled_object_count = modeled_objects.len();
            let object_types = if let Some(client) = crate::ontology_client::global() {
                client
                    .list_object_types(&session.bearer_token)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|object_type| ObjectTypeOption {
                        id: object_type.id,
                        name: object_type.name,
                    })
                    .collect()
            } else {
                vec![]
            };
            Html(
                DataDetailTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    notice: query.notice,
                    object_types,
                    record: Some(record),
                    raw_payload_pretty,
                    normalized_payload_pretty,
                    downstream_events,
                    modeled_objects,
                    error: None,
                    raw_field_count,
                    normalized_field_count,
                    downstream_event_count,
                    modeled_object_count,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Ok(None) => Html(
            DataDetailTemplate {
                show_nav: true,
                is_admin,
                can_write,
                notice: query.notice,
                object_types: vec![],
                record: None,
                raw_payload_pretty: String::new(),
                normalized_payload_pretty: None,
                downstream_events: vec![],
                modeled_objects: vec![],
                error: Some("no record with that id".to_string()),
                raw_field_count: 0,
                normalized_field_count: 0,
                downstream_event_count: 0,
                modeled_object_count: 0,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            DataDetailTemplate {
                show_nav: true,
                is_admin,
                can_write,
                notice: query.notice,
                object_types: vec![],
                record: None,
                raw_payload_pretty: String::new(),
                normalized_payload_pretty: None,
                downstream_events: vec![],
                modeled_objects: vec![],
                error: Some(e.to_string()),
                raw_field_count: 0,
                normalized_field_count: 0,
                downstream_event_count: 0,
                modeled_object_count: 0,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ModelRecordForm {
    pub object_type_id: Uuid,
    pub properties: String,
}

/// POST /data/:id/model — creates one ontology object directly from source evidence while
/// preserving the record id as source lineage. The ontology service still owns the model
/// contract and validates the submitted properties at its write boundary.
pub async fn post_model_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<ModelRecordForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let properties = match serde_json::from_str::<serde_json::Value>(&form.properties) {
        Ok(value) if value.is_object() => value,
        _ => {
            return axum::response::Redirect::to(&format!("/data/{id}?notice=model-invalid"))
                .into_response()
        }
    };
    let Some(client) = crate::ontology_client::global() else {
        return axum::response::Redirect::to(&format!("/data/{id}?notice=model-failed"))
            .into_response();
    };
    let input = CreateObjectRequest {
        object_type_id: form.object_type_id,
        properties,
        source_lineage: serde_json::json!([id]),
    };
    match client.create_object(&session.bearer_token, &input).await {
        Ok(()) => {
            axum::response::Redirect::to(&format!("/data/{id}?notice=modeled")).into_response()
        }
        Err(_) => {
            axum::response::Redirect::to(&format!("/data/{id}?notice=model-failed")).into_response()
        }
    }
}

/// POST /data/:id/reprocess — targeted source recovery from the record evidence page.
pub async fn post_reprocess_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    match state.stats_client.reprocess_record(session.tenant_id, id).await {
        Ok(count) if count > 0 => {
            axum::response::Redirect::to(&format!("/data/{id}?notice=reprocessed")).into_response()
        }
        Ok(_) => axum::response::Redirect::to(&format!("/data/{id}?notice=already-normalized"))
            .into_response(),
        Err(_) => axum::response::Redirect::to(&format!("/data/{id}?notice=reprocess-failed"))
            .into_response(),
    }
}
