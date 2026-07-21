use super::*;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use common::IncidentSeverity;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryIncidentsClient {
    pub incidents: Mutex<Vec<IncidentDetail>>,
    pub created: Mutex<Vec<Incident>>,
    pub updated: Mutex<Vec<Incident>>,
    pub linked: Mutex<Vec<(Uuid, Uuid)>>,
    pub unlinked: Mutex<Vec<(Uuid, Uuid)>>,
}

#[async_trait]
impl IncidentsClient for InMemoryIncidentsClient {
    async fn list_incidents(
        &self,
        _tenant_id: Uuid,
        status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<IncidentDetail>, IncidentsClientError> {
        Ok(self
            .incidents
            .lock()
            .unwrap()
            .iter()
            .filter(|d| status_filter.map(|s| d.incident.status == s).unwrap_or(true))
            .cloned()
            .collect())
    }

    async fn get_incident(
        &self,
        _tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<IncidentDetail>, IncidentsClientError> {
        Ok(self.incidents.lock().unwrap().iter().find(|d| d.incident.id == id).cloned())
    }

    async fn create_incident(
        &self,
        role: Role,
        _actor: &str,
        incident: Incident,
        _initial_event_ids: Vec<Uuid>,
    ) -> Result<IncidentDetail, IncidentsClientError> {
        if !role.at_least(Role::Operator) {
            return Err(IncidentsClientError::Rejected(403));
        }
        self.created.lock().unwrap().push(incident.clone());
        Ok(IncidentDetail { incident, event_ids: vec![] })
    }

    async fn update_incident(
        &self,
        role: Role,
        _actor: &str,
        incident: Incident,
    ) -> Result<IncidentDetail, IncidentsClientError> {
        if !role.at_least(Role::Operator) {
            return Err(IncidentsClientError::Rejected(403));
        }
        self.updated.lock().unwrap().push(incident.clone());
        Ok(IncidentDetail { incident, event_ids: vec![] })
    }

    async fn link_event(
        &self,
        role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), IncidentsClientError> {
        if !role.at_least(Role::Operator) {
            return Err(IncidentsClientError::Rejected(403));
        }
        self.linked.lock().unwrap().push((incident_id, event_id));
        Ok(())
    }

    async fn unlink_event(
        &self,
        role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), IncidentsClientError> {
        if !role.at_least(Role::Operator) {
            return Err(IncidentsClientError::Rejected(403));
        }
        self.unlinked.lock().unwrap().push((incident_id, event_id));
        Ok(())
    }
}

pub struct FailingIncidentsClient;

#[async_trait]
impl IncidentsClient for FailingIncidentsClient {
    async fn list_incidents(
        &self,
        _tenant_id: Uuid,
        _status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<IncidentDetail>, IncidentsClientError> {
        Err(IncidentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn get_incident(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<IncidentDetail>, IncidentsClientError> {
        Err(IncidentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn create_incident(
        &self,
        _role: Role,
        _actor: &str,
        _incident: Incident,
        _initial_event_ids: Vec<Uuid>,
    ) -> Result<IncidentDetail, IncidentsClientError> {
        Err(IncidentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn update_incident(
        &self,
        _role: Role,
        _actor: &str,
        _incident: Incident,
    ) -> Result<IncidentDetail, IncidentsClientError> {
        Err(IncidentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn link_event(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        _incident_id: Uuid,
        _event_id: Uuid,
    ) -> Result<(), IncidentsClientError> {
        Err(IncidentsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn unlink_event(
        &self,
        _role: Role,
        _actor: &str,
        _tenant_id: Uuid,
        _incident_id: Uuid,
        _event_id: Uuid,
    ) -> Result<(), IncidentsClientError> {
        Err(IncidentsClientError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_incident() -> Incident {
    Incident {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        title: "elevated error rate".to_string(),
        summary: String::new(),
        severity: IncidentSeverity::High,
        status: IncidentStatus::Open,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        resolved_at: None,
    }
}

async fn spawn_stub_server() -> String {
    async fn list_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!([{
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "title": "elevated error rate",
            "summary": "",
            "severity": "high",
            "status": "open",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "resolved_at": null,
            "event_ids": []
        }]))
        .into_response()
    }
    async fn create_handler(
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        let mut body = body;
        body["event_ids"] = serde_json::json!([]);
        (axum::http::StatusCode::CREATED, Json(body)).into_response()
    }
    async fn get_one_handler(Path(id): Path<Uuid>, headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-tenant-id").is_none() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        if id.to_string() == "99999999-9999-9999-9999-999999999999" {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        }
        Json(serde_json::json!({
            "id": id,
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "title": "elevated error rate",
            "summary": "",
            "severity": "high",
            "status": "open",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "resolved_at": null,
            "event_ids": []
        }))
        .into_response()
    }
    async fn update_handler(
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        let mut body = body;
        body["event_ids"] = serde_json::json!([]);
        Json(body).into_response()
    }
    async fn link_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        axum::http::StatusCode::NO_CONTENT.into_response()
    }
    async fn unlink_handler(headers: HeaderMap) -> axum::response::Response {
        if headers.get("x-role").and_then(|v| v.to_str().ok()) != Some("operator") {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
        axum::http::StatusCode::NO_CONTENT.into_response()
    }
    let app = Router::new()
        .route("/v1/incidents", get(list_handler).post(create_handler))
        .route("/v1/incidents/:id", get(get_one_handler).put(update_handler))
        .route("/v1/incidents/:id/events", post(link_handler))
        .route("/v1/incidents/:id/events/:event_id", axum::routing::delete(unlink_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_incidents_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);

    let incidents = client.list_incidents(Uuid::new_v4(), None).await.unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].incident.title, "elevated error rate");
}

#[tokio::test]
async fn http_client_gets_an_incident_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);
    let id = Uuid::new_v4();

    let detail = client.get_incident(Uuid::new_v4(), id).await.unwrap().unwrap();
    assert_eq!(detail.incident.id, id);
}

#[tokio::test]
async fn http_client_returns_none_when_the_incident_is_not_found() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);
    let missing_id = "99999999-9999-9999-9999-999999999999".parse().unwrap();

    let detail = client.get_incident(Uuid::new_v4(), missing_id).await.unwrap();
    assert!(detail.is_none());
}

#[tokio::test]
async fn http_client_creates_an_incident_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);

    let created =
        client.create_incident(Role::Operator, "alice", sample_incident(), vec![]).await.unwrap();
    assert_eq!(created.incident.title, "elevated error rate");
}

#[tokio::test]
async fn http_client_create_is_rejected_for_insufficient_role() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);

    let err =
        client.create_incident(Role::Viewer, "alice", sample_incident(), vec![]).await.unwrap_err();
    assert!(matches!(err, IncidentsClientError::Rejected(403)));
}

#[tokio::test]
async fn http_client_updates_an_incident_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);
    let mut incident = sample_incident();
    incident.status = IncidentStatus::Resolved;

    let updated = client.update_incident(Role::Operator, "alice", incident).await.unwrap();
    assert_eq!(updated.incident.status, IncidentStatus::Resolved);
}

#[tokio::test]
async fn http_client_links_an_event_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);

    client
        .link_event(Role::Operator, "alice", Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
        .await
        .unwrap();
}

#[tokio::test]
async fn http_client_unlinks_an_event_against_a_real_server() {
    let url = spawn_stub_server().await;
    let client = HttpIncidentsClient::new(reqwest::Client::new(), url);

    client
        .unlink_event(Role::Operator, "alice", Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
        .await
        .unwrap();
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpIncidentsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_incidents(Uuid::new_v4(), None).await.unwrap_err();
    assert!(matches!(err, IncidentsClientError::Unreachable(_)));
}
