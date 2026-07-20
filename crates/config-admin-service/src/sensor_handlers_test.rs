use super::*;
use crate::sensor_publisher::sensor_publisher_test::InMemorySensorPublisher;
use crate::sensor_repository::sensor_repository_test::{
    FailingSensorRepository, InMemorySensorRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use tower::ServiceExt;

fn router(state: SensorState) -> Router {
    Router::new()
        .route("/v1/sensors", post(create_sensor).get(list_sensors))
        .route("/v1/sensors/by-name/:name", get(get_sensor_by_name))
        .route("/v1/sensors/:id", get(get_sensor).put(update_sensor).delete(delete_sensor))
        .with_state(state)
}

fn sample_sensor(tenant_id: Uuid) -> Sensor {
    Sensor::new(
        tenant_id,
        "zendesk",
        "support-poller",
        serde_json::json!({"url": "https://example.zendesk.com"}),
    )
}

fn default_state() -> SensorState {
    SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::default()),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    }
}

async fn send(
    app: Router,
    method: &str,
    uri: String,
    tenant_header: Option<Uuid>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    send_with_role(app, method, uri, tenant_header, Some("operator"), body).await
}

#[allow(clippy::too_many_arguments)]
async fn send_with_role(
    app: Router,
    method: &str,
    uri: String,
    tenant_header: Option<Uuid>,
    role_header: Option<&str>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(tenant_id) = tenant_header {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    if let Some(role) = role_header {
        req = req.header("x-role", role);
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_sensor_succeeds_when_tenant_matches() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/sensors".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&sensor).unwrap()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_sensor_is_rejected_when_tenant_does_not_match() {
    let sensor = sample_sensor(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "POST",
        "/v1/sensors".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&sensor).unwrap()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn update_sensor_is_rejected_when_tenant_does_not_match() {
    let sensor = sample_sensor(Uuid::new_v4());
    let response = send(
        router(default_state()),
        "PUT",
        format!("/v1/sensors/{}", sensor.id),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&sensor).unwrap()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_sensors_is_scoped_to_the_header_tenant() {
    let tenant_id = Uuid::new_v4();
    let state = SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::with_sensor(sample_sensor(
            tenant_id,
        ))),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };
    let response =
        send(router(state), "GET", "/v1/sensors".to_string(), Some(tenant_id), None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sensors: Vec<Sensor> = serde_json::from_value(body["sensors"].clone()).unwrap();
    assert_eq!(sensors.len(), 1);
    assert_eq!(body["has_more"], serde_json::json!(false));
}

#[tokio::test]
async fn list_sensors_reports_has_more_when_results_exceed_the_page_size() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySensorRepository::default();
    for name in ["a", "b", "c"] {
        repo.create(Sensor::new(tenant_id, "zendesk", name, serde_json::json!({}))).await.unwrap();
    }
    let state = SensorState {
        sensor_repository: Arc::new(repo),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };

    let response =
        send(router(state), "GET", "/v1/sensors?limit=2".to_string(), Some(tenant_id), None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["sensors"].as_array().unwrap().len(), 2);
    assert_eq!(body["has_more"], serde_json::json!(true));
}

#[tokio::test]
async fn get_sensor_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/sensors/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_sensor_succeeds_then_get_returns_404() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let state = SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::with_sensor(sensor.clone())),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };
    let app = router(state);

    let delete_response =
        send(app.clone(), "DELETE", format!("/v1/sensors/{}", sensor.id), Some(tenant_id), None)
            .await;
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let get_response =
        send(app, "GET", format!("/v1/sensors/{}", sensor.id), Some(tenant_id), None).await;
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn repository_failure_returns_500() {
    let tenant_id = Uuid::new_v4();
    let state = SensorState {
        sensor_repository: Arc::new(FailingSensorRepository),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };
    let response =
        send(router(state), "GET", "/v1/sensors".to_string(), Some(tenant_id), None).await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn get_sensor_by_name_returns_the_matching_sensor() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let state = SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::with_sensor(sensor.clone())),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };

    let response = send(
        router(state),
        "GET",
        "/v1/sensors/by-name/support-poller".to_string(),
        Some(tenant_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: Sensor = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(found, sensor);
}

#[tokio::test]
async fn get_sensor_by_name_returns_404_for_unknown_name() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "GET",
        "/v1/sensors/by-name/nonexistent".to_string(),
        Some(tenant_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// --- RBAC (ADR-0016 follow-up): sensor write endpoints must enforce the same Operator-minimum
// role check trigger-definition and normalization-mapping writes already have. Previously these
// three handlers never called `require_operator` at all — any authenticated Viewer-role session
// (or anyone hitting the API directly) could register/update/delete another tenant's Sensors.

#[tokio::test]
async fn create_sensor_requires_role_header() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let response = send_with_role(
        router(default_state()),
        "POST",
        "/v1/sensors".to_string(),
        Some(tenant_id),
        None,
        Some(serde_json::to_value(&sensor).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_sensor_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let response = send_with_role(
        router(default_state()),
        "POST",
        "/v1/sensors".to_string(),
        Some(tenant_id),
        Some("viewer"),
        Some(serde_json::to_value(&sensor).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_sensor_allows_an_operator_role() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let response = send_with_role(
        router(default_state()),
        "POST",
        "/v1/sensors".to_string(),
        Some(tenant_id),
        Some("operator"),
        Some(serde_json::to_value(&sensor).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn update_sensor_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let state = SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::with_sensor(sensor.clone())),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };
    let mut updated = sensor.clone();
    updated.enabled = false;
    let response = send_with_role(
        router(state),
        "PUT",
        format!("/v1/sensors/{}", sensor.id),
        Some(tenant_id),
        Some("viewer"),
        Some(serde_json::to_value(&updated).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_sensor_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let state = SensorState {
        sensor_repository: Arc::new(InMemorySensorRepository::with_sensor(sensor.clone())),
        sensor_publisher: Arc::new(InMemorySensorPublisher::default()),
    };
    let response = send_with_role(
        router(state),
        "DELETE",
        format!("/v1/sensors/{}", sensor.id),
        Some(tenant_id),
        Some("viewer"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
