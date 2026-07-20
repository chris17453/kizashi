#[path = "sensor_detail_handler_test.rs"]
#[cfg(test)]
mod sensor_detail_handler_test;

use crate::session_guard::require_session;
use crate::{AppState, RecordSummary};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use common::Sensor;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "sensor_detail.html")]
struct SensorDetailTemplate {
    show_nav: bool,
    sensor: Option<Sensor>,
    records: Vec<RecordSummary>,
    error: Option<String>,
}

/// GET /sensors/:id — the per-sensor data drill-down: the sensor's own registration plus the
/// most recent raw records its connector has ingested (matched on `sensor.name ==
/// record.connector_id`, same convention as the Sensors list's status column).
pub async fn get_sensor_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let sensor = match state.sensors_client.get_sensor(session.tenant_id, id).await {
        Ok(Some(sensor)) => sensor,
        Ok(None) => {
            return Html(
                SensorDetailTemplate {
                    show_nav: true,
                    sensor: None,
                    records: vec![],
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
                    sensor: None,
                    records: vec![],
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

    Html(
        SensorDetailTemplate { show_nav: true, sensor: Some(sensor), records, error: None }
            .render()
            .unwrap(),
    )
    .into_response()
}
