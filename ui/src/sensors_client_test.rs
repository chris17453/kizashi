use super::*;
use axum::extract::Json as JsonExtractor;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySensorsClient {
    pub sensors: Mutex<Vec<Sensor>>,
    pub has_more: Mutex<bool>,
}

#[async_trait]
impl SensorsClient for InMemorySensorsClient {
    async fn list_sensors(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<SensorsPage, SensorsClientError> {
        Ok(SensorsPage {
            sensors: self.sensors.lock().unwrap().clone(),
            has_more: *self.has_more.lock().unwrap(),
        })
    }

    async fn get_sensor(
        &self,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sensor>, SensorsClientError> {
        Ok(self.sensors.lock().unwrap().iter().find(|a| a.id == id).cloned())
    }

    async fn register_sensor(
        &self,
        _role: Role,
        _actor: &str,
        tenant_id: Uuid,
        connector_type: &str,
        name: &str,
        config: serde_json::Value,
    ) -> Result<Sensor, SensorsClientError> {
        let sensor = Sensor::new(tenant_id, connector_type, name, config);
        self.sensors.lock().unwrap().push(sensor.clone());
        Ok(sensor)
    }

    async fn delete_sensor(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), SensorsClientError> {
        self.sensors.lock().unwrap().retain(|a| a.id != id);
        Ok(())
    }

    async fn update_sensor(
        &self,
        _role: Role,
        _actor: &str,
        sensor: &Sensor,
    ) -> Result<Sensor, SensorsClientError> {
        let mut sensors = self.sensors.lock().unwrap();
        match sensors.iter_mut().find(|a| a.id == sensor.id) {
            Some(existing) => {
                *existing = sensor.clone();
                Ok(sensor.clone())
            }
            None => Err(SensorsClientError::Rejected(404)),
        }
    }
}

pub struct FailingSensorsClient;

#[async_trait]
impl SensorsClient for FailingSensorsClient {
    async fn list_sensors(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<SensorsPage, SensorsClientError> {
        Err(SensorsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn get_sensor(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Sensor>, SensorsClientError> {
        Err(SensorsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn register_sensor(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        _connector_type: &str,
        _name: &str,
        _config: serde_json::Value,
    ) -> Result<Sensor, SensorsClientError> {
        Err(SensorsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn delete_sensor(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<(), SensorsClientError> {
        Err(SensorsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn update_sensor(
        &self,
        _role: Role,
        _actor: &str,
        _sensor: &Sensor,
    ) -> Result<Sensor, SensorsClientError> {
        Err(SensorsClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server() -> String {
    async fn list_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({
            "sensors": [{
                "id": "11111111-1111-1111-1111-111111111111",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "connector_type": "zendesk",
                "name": "support-poller",
                "config": {},
                "enabled": true
            }],
            "has_more": false
        }))
        .into_response()
    }
    async fn get_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "connector_type": "zendesk",
            "name": "support-poller",
            "config": {},
            "enabled": true
        }))
        .into_response()
    }
    async fn create_handler(
        headers: HeaderMap,
        JsonExtractor(sensor): JsonExtractor<Sensor>,
    ) -> axum::response::Response {
        if headers.get("x-username").and_then(|v| v.to_str().ok()) != Some("alice") {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        (axum::http::StatusCode::CREATED, Json(sensor)).into_response()
    }
    async fn delete_handler(headers: HeaderMap) -> axum::http::StatusCode {
        if headers.get("x-username").and_then(|v| v.to_str().ok()) != Some("alice") {
            return axum::http::StatusCode::UNAUTHORIZED;
        }
        axum::http::StatusCode::NO_CONTENT
    }
    async fn update_handler(
        headers: HeaderMap,
        JsonExtractor(sensor): JsonExtractor<Sensor>,
    ) -> axum::response::Response {
        if headers.get("x-username").and_then(|v| v.to_str().ok()) != Some("alice") {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(sensor).into_response()
    }
    let app = Router::new()
        .route("/v1/sensors", get(list_handler).post(create_handler))
        .route("/v1/sensors/:id", get(get_handler).delete(delete_handler).put(update_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_sensors_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSensorsClient::new(reqwest::Client::new(), url);

    let page = client.list_sensors(Uuid::new_v4(), 25, 0).await.unwrap();

    assert_eq!(page.sensors.len(), 1);
    assert_eq!(page.sensors[0].name, "support-poller");
    assert!(!page.has_more);
}

#[tokio::test]
async fn http_client_gets_a_sensor_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSensorsClient::new(reqwest::Client::new(), url);

    let sensor = client.get_sensor(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();

    assert!(sensor.is_some());
    assert_eq!(sensor.unwrap().name, "support-poller");
}

#[tokio::test]
async fn http_client_registers_a_sensor_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSensorsClient::new(reqwest::Client::new(), url);
    let tenant_id = Uuid::new_v4();

    let sensor = client
        .register_sensor(
            Role::Operator,
            "alice",
            tenant_id,
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap();

    assert_eq!(sensor.tenant_id, tenant_id);
    assert_eq!(sensor.name, "support-poller");
}

#[tokio::test]
async fn http_client_deletes_a_sensor_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSensorsClient::new(reqwest::Client::new(), url);

    client.delete_sensor(Role::Operator, "alice", Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn http_client_updates_a_sensor_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpSensorsClient::new(reqwest::Client::new(), url);
    let mut sensor =
        Sensor::new(Uuid::new_v4(), "zendesk", "support-poller", serde_json::json!({}));
    sensor.enabled = false;

    let updated = client.update_sensor(Role::Operator, "alice", &sensor).await.unwrap();
    assert!(!updated.enabled);
}

#[tokio::test]
async fn http_client_register_sensor_is_rejected_when_actor_header_missing_expected_value() {
    let url = spawn_stub_server().await;
    let client = HttpSensorsClient::new(reqwest::Client::new(), url);

    let err = client
        .register_sensor(
            Role::Operator,
            "someone-else",
            Uuid::new_v4(),
            "zendesk",
            "support-poller",
            serde_json::json!({}),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, SensorsClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpSensorsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_sensors(Uuid::new_v4(), 25, 0).await.unwrap_err();
    assert!(matches!(err, SensorsClientError::Unreachable(_)));
}
