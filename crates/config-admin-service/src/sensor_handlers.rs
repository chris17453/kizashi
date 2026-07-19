#[path = "sensor_handlers_test.rs"]
#[cfg(test)]
mod sensor_handlers_test;

use crate::handlers::{require_operator, tenant_id_from_headers, tenant_mismatch};
use crate::sensor_publisher::SensorPublisher;
use crate::sensor_repository::{SensorRepository, SensorRepositoryError};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::{Sensor, SensorChangeEvent};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct SensorState {
    pub sensor_repository: Arc<dyn SensorRepository>,
    pub sensor_publisher: Arc<dyn SensorPublisher>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn sensor_error_response(e: SensorRepositoryError) -> Response {
    match e {
        SensorRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no sensor with id {id}"))
        }
        SensorRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

/// POST /v1/sensors — registers a new sensor (a connector instance for a tenant). This is the
/// entity that never existed before: previously the 6 connector binaries were configured only
/// by env vars, with no service that knew of their existence.
pub async fn create_sensor(
    State(state): State<SensorState>,
    headers: HeaderMap,
    Json(sensor): Json<Sensor>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, sensor.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    match state.sensor_repository.create(sensor).await {
        Ok(created) => {
            let event = SensorChangeEvent::Upserted(created.clone());
            if let Err(e) = state.sensor_publisher.publish_sensor_changed(&event).await {
                tracing::error!(sensor_id = %created.id, error = %e, "failed to publish sensor.changed");
            }
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => sensor_error_response(e),
    }
}

pub async fn update_sensor(
    State(state): State<SensorState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut sensor): Json<Sensor>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, sensor.tenant_id) {
        return response;
    }
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    sensor.id = id;
    match state.sensor_repository.update(sensor).await {
        Ok(updated) => {
            let event = SensorChangeEvent::Upserted(updated.clone());
            if let Err(e) = state.sensor_publisher.publish_sensor_changed(&event).await {
                tracing::error!(sensor_id = %updated.id, error = %e, "failed to publish sensor.changed");
            }
            Json(updated).into_response()
        }
        Err(e) => sensor_error_response(e),
    }
}

pub async fn get_sensor(
    State(state): State<SensorState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.sensor_repository.get(tenant_id, id).await {
        Ok(Some(sensor)) => Json(sensor).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("no sensor with id {id}")),
        Err(e) => sensor_error_response(e),
    }
}

/// GET /v1/sensors/by-name/:name — the lookup Ingestion Gateway uses to enforce a sensor's
/// enabled/disabled status at ingest time, matched on the same `name == connector_id`
/// convention `SensorsClient` documents.
pub async fn get_sensor_by_name(
    State(state): State<SensorState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.sensor_repository.find_by_name(tenant_id, &name).await {
        Ok(Some(sensor)) => Json(sensor).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, format!("no sensor named {name}")),
        Err(e) => sensor_error_response(e),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ListSensorsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    25
}

#[derive(Debug, serde::Serialize)]
pub struct ListSensorsResponse {
    pub sensors: Vec<Sensor>,
    pub has_more: bool,
}

pub async fn list_sensors(
    State(state): State<SensorState>,
    headers: HeaderMap,
    Query(query): Query<ListSensorsQuery>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.sensor_repository.list(tenant_id, query.limit + 1, query.offset).await {
        Ok(mut sensors) => {
            let has_more = sensors.len() as i64 > query.limit;
            sensors.truncate(query.limit as usize);
            Json(ListSensorsResponse { sensors, has_more }).into_response()
        }
        Err(e) => sensor_error_response(e),
    }
}

pub async fn delete_sensor(
    State(state): State<SensorState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    if let Some(response) = require_operator(&headers) {
        return response;
    }
    match state.sensor_repository.delete(tenant_id, id).await {
        Ok(()) => {
            let event = SensorChangeEvent::Deleted { id, tenant_id };
            if let Err(e) = state.sensor_publisher.publish_sensor_changed(&event).await {
                tracing::error!(sensor_id = %id, error = %e, "failed to publish sensor.changed");
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => sensor_error_response(e),
    }
}
