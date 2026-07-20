#[path = "sensors_handler_mutations_test.rs"]
#[cfg(test)]
mod sensors_handler_mutations_test;
#[path = "sensors_handler_pagination_test.rs"]
#[cfg(test)]
mod sensors_handler_pagination_test;
#[path = "sensors_handler_test.rs"]
#[cfg(test)]
mod sensors_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, ConnectorStatSummary};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use common::Sensor;
use uuid::Uuid;

/// One row of the Sensors table: a registered `Sensor` joined against Ingestion Service's
/// per-connector stats, matched on `sensor.name == connector_stats.connector_id` (the
/// operational convention `SensorsClient` documents — a sensor's registered `name` is what the
/// deployed connector's `CONNECTOR_ID` env var is set to). No match means the sensor has never
/// ingested anything yet, not an error.
struct SensorRow {
    id: Uuid,
    connector_type: String,
    name: String,
    enabled: bool,
    record_count: Option<i64>,
    last_ingested_at: Option<DateTime<Utc>>,
}

fn join_sensor_stats(sensors: Vec<Sensor>, stats: Vec<ConnectorStatSummary>) -> Vec<SensorRow> {
    sensors
        .into_iter()
        .map(|sensor| {
            let matched = stats.iter().find(|s| s.connector_id == sensor.name);
            SensorRow {
                id: sensor.id,
                connector_type: sensor.connector_type,
                name: sensor.name,
                enabled: sensor.enabled,
                record_count: matched.map(|s| s.record_count),
                last_ingested_at: matched.map(|s| s.last_ingested_at),
            }
        })
        .collect()
}

fn default_page() -> i64 {
    0
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct SensorsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub dir: String,
}

/// Case-insensitive substring match on name -- same shape as Triggers' search (ADR-0066). Like
/// Triggers, `list_sensors` is server-paginated, so this only filters the *current page's*
/// already-fetched sensors, not the tenant's full set.
fn matches_query(row: &SensorRow, q: &str) -> bool {
    q.is_empty() || row.name.to_lowercase().contains(&q.to_lowercase())
}

/// Same shape as Triggers' sortable columns (ADR-0070), applied after the search filter and,
/// like search, only reordering the current page.
fn sort_rows(rows: &mut [SensorRow], sort: &str, dir: &str) {
    match sort {
        "connector_type" => rows.sort_by_key(|r| r.connector_type.to_lowercase()),
        "enabled" => rows.sort_by_key(|r| !r.enabled),
        _ => rows.sort_by_key(|r| r.name.to_lowercase()),
    }
    if dir == "desc" {
        rows.reverse();
    }
}

#[derive(Template)]
#[template(path = "sensors.html")]
struct SensorsTemplate {
    show_nav: bool,
    is_admin: bool,
    sensors: Vec<SensorRow>,
    page: i64,
    has_more: bool,
    /// RBAC v1 (ADR-0016): hides the register form and enable/disable/remove buttons from a
    /// `Viewer` — the backend doesn't enforce this particular write path yet (only
    /// config-admin-service's trigger/mapping writes and retention-service's policy writes do),
    /// so this is presentation-layer only for now, not a substitute for server-side gating.
    can_write: bool,
    error: Option<String>,
    q: String,
    sort: String,
    dir: String,
}

pub async fn get_sensors(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SensorsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let page = query.page.max(0);
    let result = state
        .sensors_client
        .list_sensors(session.tenant_id, DEFAULT_PAGE_SIZE, page * DEFAULT_PAGE_SIZE)
        .await;
    let (sensors, has_more) = match result {
        Ok(page_result) => (page_result.sensors, page_result.has_more),
        Err(e) => {
            return Html(
                SensorsTemplate {
                    show_nav: true,
                    is_admin,
                    sensors: vec![],
                    page,
                    has_more: false,
                    can_write,
                    error: Some(e.to_string()),
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let stats = state.stats_client.connector_stats(session.tenant_id).await.unwrap_or_default();
    let mut rows: Vec<SensorRow> = join_sensor_stats(sensors, stats)
        .into_iter()
        .filter(|r| matches_query(r, &query.q))
        .collect();
    sort_rows(&mut rows, &query.sort, &query.dir);

    Html(
        SensorsTemplate {
            show_nav: true,
            is_admin,
            sensors: rows,
            page,
            has_more,
            can_write,
            error: None,
            q: query.q,
            sort: query.sort,
            dir: query.dir,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct RegisterSensorForm {
    connector_type: String,
    name: String,
    #[serde(default)]
    config: String,
}

async fn rerender_with_error(
    state: &AppState,
    tenant_id: Uuid,
    is_admin: bool,
    can_write: bool,
    error: String,
) -> Response {
    let sensors = state
        .sensors_client
        .list_sensors(tenant_id, DEFAULT_PAGE_SIZE, 0)
        .await
        .map(|p| p.sensors)
        .unwrap_or_default();
    let stats = state.stats_client.connector_stats(tenant_id).await.unwrap_or_default();
    Html(
        SensorsTemplate {
            show_nav: true,
            is_admin,
            sensors: join_sensor_stats(sensors, stats),
            page: 0,
            has_more: false,
            can_write,
            error: Some(error),
            q: String::new(),
            sort: String::new(),
            dir: String::new(),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn post_sensors(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<RegisterSensorForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let is_admin = session.role.at_least(common::Role::Admin);

    let config: serde_json::Value = if form.config.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&form.config) {
            Ok(value) => value,
            Err(_) => {
                return rerender_with_error(
                    &state,
                    session.tenant_id,
                    is_admin,
                    session.role.at_least(common::Role::Operator),
                    "config must be valid JSON".to_string(),
                )
                .await;
            }
        }
    };

    if let Err(e) = state
        .sensors_client
        .register_sensor(
            session.role,
            &session.username,
            session.tenant_id,
            &form.connector_type,
            &form.name,
            config,
        )
        .await
    {
        return rerender_with_error(
            &state,
            session.tenant_id,
            is_admin,
            session.role.at_least(common::Role::Operator),
            e.to_string(),
        )
        .await;
    }

    Redirect::to("/sensors").into_response()
}

/// `axum::extract::Form` deserializes via `serde_urlencoded`, which -- unlike some other form
/// crates -- does NOT collect repeated same-named fields (one checkbox per row, all named
/// `ids`) into a `Vec`; it only supports flat scalar struct fields. Parsing the raw body as a
/// flat list of `(key, value)` pairs instead and filtering for `"ids"` sidesteps that limitation
/// without adding a new dependency (`serde_urlencoded` is already a direct dependency). Same
/// pattern as API Keys' `post_bulk_revoke_api_keys` (ADR-0065).
fn parse_ids(raw_body: &[u8]) -> Vec<Uuid> {
    let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(raw_body) else {
        return Vec::new();
    };
    pairs
        .into_iter()
        .filter(|(key, _)| key == "ids")
        .filter_map(|(_, value)| value.parse::<Uuid>().ok())
        .collect()
}

/// POST /sensors/bulk-delete — removes every selected sensor (same bulk-action pattern API
/// Keys already has, ADR-0065: loop over the existing single-item `SensorsClient::delete_sensor`
/// rather than a new bulk backend endpoint). Best-effort per sensor, same as the single-delete
/// handler above. Empty (nothing selected) is a legitimate no-op, not an error.
pub async fn post_bulk_delete_sensors(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    for id in parse_ids(&body) {
        let _ = state
            .sensors_client
            .delete_sensor(session.role, &session.username, session.tenant_id, id)
            .await;
    }
    Redirect::to("/sensors").into_response()
}

pub async fn post_delete_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let _ = state
        .sensors_client
        .delete_sensor(session.role, &session.username, session.tenant_id, id)
        .await;
    Redirect::to("/sensors").into_response()
}

/// POST /sensors/:id/toggle — flips a sensor's enabled/disabled status. This is the one place
/// that flag actually does something: Ingestion Gateway checks it on every ingest and rejects
/// a disabled sensor's data (previously stored but never enforced anywhere).
pub async fn post_toggle_sensor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return StatusCode::FORBIDDEN.into_response();
    }

    if let Ok(Some(mut sensor)) = state.sensors_client.get_sensor(session.tenant_id, id).await {
        sensor.enabled = !sensor.enabled;
        let _ = state.sensors_client.update_sensor(session.role, &session.username, &sensor).await;
    }
    Redirect::to("/sensors").into_response()
}
