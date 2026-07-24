#[path = "sensor_detail_handler_test.rs"]
#[cfg(test)]
mod sensor_detail_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use common::Sensor;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "sensor_detail.html")]
struct SensorDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    sensor: Option<Sensor>,
    records: Vec<RecordSummary>,
    total_records: i64,
    last_ingested_at: Option<chrono::DateTime<chrono::Utc>>,
    connector_health: String,
    normalized_records: usize,
    downstream_events: Vec<SensorEventContext>,
    downstream_incidents: Vec<SensorIncidentContext>,
    modeled_objects: Vec<SensorObjectContext>,
    downstream_actions: Vec<SensorActionContext>,
    activity_bars: Vec<SensorActivityBar>,
    config_pretty: String,
    notice: String,
    error: Option<String>,
}

struct SensorEventContext {
    id: Uuid,
    event_type: String,
    status: String,
    occurred_at: chrono::DateTime<chrono::Utc>,
}

struct SensorObjectContext {
    id: Uuid,
    type_name: String,
    label: String,
}

struct SensorIncidentContext {
    id: Uuid,
    title: String,
    severity: String,
    status: String,
    event_count: usize,
}

struct SensorActionContext {
    id: Uuid,
    action_name: String,
    target_label: String,
    outcome: String,
    executed_at: chrono::DateTime<chrono::Utc>,
}

struct SensorActivityBar {
    date: String,
    count: usize,
    height_pct: usize,
    href: String,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct SensorDetailQuery {
    #[serde(default)]
    pub notice: String,
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn activity_bars(records: &[RecordSummary]) -> Vec<SensorActivityBar> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for record in records {
        *counts.entry(record.ingested_at.date_naive().to_string()).or_default() += 1;
    }
    let max = counts.values().copied().max().unwrap_or(0);
    counts
        .into_iter()
        .map(|(date, count)| {
            let href = serde_urlencoded::to_string([
                (
                    "connector_id",
                    records.first().map(|record| record.connector_id.clone()).unwrap_or_default(),
                ),
                ("from", date.clone()),
                ("to", date.clone()),
            ])
            .map(|query| format!("/data?{query}"))
            .unwrap_or_else(|_| "/data".to_string());
            SensorActivityBar {
                date,
                count,
                height_pct: if max == 0 { 0 } else { (count * 100 / max).max(8) },
                href,
            }
        })
        .collect()
}

/// GET /sensors/:id — the per-sensor data drill-down: the sensor's own registration plus the
/// most recent raw records its connector has ingested (matched on `sensor.name ==
/// record.connector_id`, same convention as the Sensors list's status column).
pub async fn get_sensor_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<SensorDetailQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let sensor = match state.sensors_client.get_sensor(session.tenant_id, id).await {
        Ok(Some(sensor)) => sensor,
        Ok(None) => {
            return Html(
                SensorDetailTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    sensor: None,
                    records: vec![],
                    total_records: 0,
                    last_ingested_at: None,
                    connector_health: "unknown".to_string(),
                    normalized_records: 0,
                    downstream_events: vec![],
                    downstream_incidents: vec![],
                    modeled_objects: vec![],
                    downstream_actions: vec![],
                    activity_bars: vec![],
                    config_pretty: String::new(),
                    notice: query.notice,
                    error: Some("no sensor with that id".to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
        Err(e) => {
            return Html(
                SensorDetailTemplate {
                    show_nav: true,
                    is_admin,
                    can_write,
                    sensor: None,
                    records: vec![],
                    total_records: 0,
                    last_ingested_at: None,
                    connector_health: "unknown".to_string(),
                    normalized_records: 0,
                    downstream_events: vec![],
                    downstream_incidents: vec![],
                    modeled_objects: vec![],
                    downstream_actions: vec![],
                    activity_bars: vec![],
                    config_pretty: String::new(),
                    notice: query.notice,
                    error: Some(e.to_string()),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let records = state
        .stats_client
        .records_by_connector(session.tenant_id, &sensor.name)
        .await
        .unwrap_or_default();
    let connector_stat = state
        .stats_client
        .connector_stats(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|stat| stat.connector_id == sensor.name);
    let total_records = connector_stat.as_ref().map(|stat| stat.record_count).unwrap_or(0);
    let last_ingested_at = connector_stat.as_ref().map(|stat| stat.last_ingested_at);
    let connector_health = if !sensor.enabled {
        "disabled"
    } else if let Some(last) = last_ingested_at {
        let age = chrono::Utc::now() - last;
        if age <= chrono::Duration::hours(1) {
            "healthy"
        } else {
            "stale"
        }
    } else {
        "no_data"
    }
    .to_string();
    let normalized_records = records.iter().filter(|record| record.is_normalized()).count();
    let activity_bars = activity_bars(&records);
    let record_ids =
        records.iter().map(|record| record.id).collect::<std::collections::HashSet<_>>();
    let downstream_events = state
        .events_client
        .list_events(&session.bearer_token, 1000, 0, None, None)
        .await
        .map(|page| page.events)
        .unwrap_or_default()
        .into_iter()
        .filter(|event| event.record_ids.iter().any(|id| record_ids.contains(id)))
        .map(|event| SensorEventContext {
            id: event.id,
            event_type: event.event_type,
            status: event.status,
            occurred_at: event.occurred_at,
        })
        .collect::<Vec<_>>();
    let downstream_event_ids =
        downstream_events.iter().map(|event| event.id).collect::<std::collections::HashSet<_>>();
    let downstream_incidents = state
        .incidents_client
        .list_incidents(session.tenant_id, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|detail| {
            let event_count = detail
                .event_ids
                .iter()
                .filter(|event_id| downstream_event_ids.contains(event_id))
                .count();
            (event_count > 0).then(|| SensorIncidentContext {
                id: detail.incident.id,
                title: detail.incident.title,
                severity: detail.incident.severity.to_string(),
                status: detail.incident.status.to_string(),
                event_count,
            })
        })
        .collect::<Vec<_>>();
    let (modeled_objects, downstream_actions) =
        if let Some(client) = crate::ontology_client::global() {
            let (types, objects, action_types, invocations) = tokio::join!(
                client.list_object_types(&session.bearer_token),
                client.list_objects(&session.bearer_token, None),
                client.list_action_types(&session.bearer_token),
                client.list_action_invocations(&session.bearer_token)
            );
            let names = types
                .unwrap_or_default()
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
                            lineage.iter().any(|value| {
                                value
                                    .as_str()
                                    .and_then(|value| value.parse::<Uuid>().ok())
                                    .is_some_and(|id| record_ids.contains(&id))
                            })
                        })
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            let object_labels = derived_objects
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
                    (object.id, label)
                })
                .collect::<std::collections::HashMap<_, _>>();
            let modeled_objects = derived_objects
                .iter()
                .map(|object| SensorObjectContext {
                    id: object.id,
                    type_name: names
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
                .collect::<Vec<_>>();
            let action_names = action_types
                .unwrap_or_default()
                .into_iter()
                .map(|action| (action.id, action.name))
                .collect::<std::collections::HashMap<_, _>>();
            let downstream_actions = invocations
                .unwrap_or_default()
                .into_iter()
                .filter_map(|invocation| {
                    let target_id = invocation
                        .target_object_ids
                        .as_array()
                        .and_then(|targets| targets.iter().find_map(|target| target.as_str()))
                        .and_then(|target| target.parse::<Uuid>().ok())?;
                    let target_label = object_labels.get(&target_id).cloned()?;
                    Some(SensorActionContext {
                        id: invocation.id,
                        action_name: action_names
                            .get(&invocation.action_type_id)
                            .cloned()
                            .unwrap_or_else(|| "Governed action".to_string()),
                        target_label,
                        outcome: invocation.outcome,
                        executed_at: invocation.executed_at,
                    })
                })
                .collect::<Vec<_>>();
            (modeled_objects, downstream_actions)
        } else {
            (vec![], vec![])
        };
    let config_pretty = pretty(&sensor.config);

    Html(
        SensorDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            sensor: Some(sensor),
            records,
            total_records,
            last_ingested_at,
            connector_health,
            normalized_records,
            downstream_events,
            downstream_incidents,
            modeled_objects,
            downstream_actions,
            activity_bars,
            config_pretty,
            notice: query.notice,
            error: None,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateSensorForm {
    pub config: String,
    pub enabled: String,
}

/// POST /sensors/:id/edit — updates the connector's opaque config and enabled state through the
/// existing Config Admin update/audit path. Connector identity remains immutable here.
pub async fn post_update_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<UpdateSensorForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let config = match serde_json::from_str::<serde_json::Value>(&form.config) {
        Ok(value) if value.is_object() => value,
        _ => {
            return axum::response::Redirect::to(&format!("/sensors/{id}?notice=config-invalid"))
                .into_response()
        }
    };
    let Ok(Some(mut sensor)) = state.sensors_client.get_sensor(session.tenant_id, id).await else {
        return axum::response::Redirect::to(&format!("/sensors/{id}?notice=update-failed"))
            .into_response();
    };
    sensor.config = config;
    sensor.enabled = form.enabled == "true";
    match state.sensors_client.update_sensor(session.role, &session.username, &sensor).await {
        Ok(_) => {
            axum::response::Redirect::to(&format!("/sensors/{id}?notice=updated")).into_response()
        }
        Err(_) => axum::response::Redirect::to(&format!("/sensors/{id}?notice=update-failed"))
            .into_response(),
    }
}
