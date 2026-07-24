use super::*;
use crate::audit_log::audit_log_test::InMemoryAuditLogReader;
use crate::incident_repository::incident_repository_test::{
    FailingIncidentRepository, InMemoryIncidentRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use tower::ServiceExt;

fn router(state: IncidentState) -> Router {
    Router::new()
        .route("/v1/audit-log", axum::routing::get(list_audit_log))
        .route("/v1/audit-log/:entity_id", axum::routing::get(list_entity_audit_log))
        .route("/v1/incidents", post(create_incident).get(list_incidents))
        .route("/v1/incidents/:id", get(get_incident).put(update_incident))
        .route("/v1/incidents/:id/notes", axum::routing::post(add_incident_note))
        .route("/v1/incidents/:id/events", post(link_event))
        .route("/v1/incidents/:id/events/:event_id", axum::routing::delete(unlink_event))
        .with_state(state)
}

fn default_state() -> IncidentState {
    IncidentState {
        incident_repository: Arc::new(InMemoryIncidentRepository::default()),
        audit_log_reader: Arc::new(InMemoryAuditLogReader::default()),
    }
}

fn sample_incident(tenant_id: Uuid) -> Incident {
    Incident {
        id: Uuid::new_v4(),
        tenant_id,
        title: "elevated error rate".to_string(),
        summary: String::new(),
        severity: common::IncidentSeverity::High,
        status: IncidentStatus::Open,
        assigned_to: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        resolved_at: None,
    }
}

async fn send(
    app: Router,
    method: &str,
    uri: String,
    tenant_header: Option<Uuid>,
    body: Option<serde_json::Value>,
) -> axum::http::Response<Body> {
    let mut req =
        Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(tenant_id) = tenant_header {
        req = req
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", "operator")
            .header("x-username", "test-actor@example.com");
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_incident_succeeds_when_tenant_matches() {
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/incidents".to_string(),
        Some(tenant_id),
        Some(serde_json::json!({
            "id": incident.id, "tenant_id": incident.tenant_id, "title": incident.title,
            "summary": incident.summary, "severity": "high", "status": "open",
            "created_at": incident.created_at, "updated_at": incident.updated_at,
            "resolved_at": null, "initial_event_ids": []
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_incident_rejects_a_tenant_mismatch() {
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/incidents".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::json!({
            "id": incident.id, "tenant_id": incident.tenant_id, "title": incident.title,
            "summary": incident.summary, "severity": "high", "status": "open",
            "created_at": incident.created_at, "updated_at": incident.updated_at,
            "resolved_at": null
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_incident_rejects_a_viewer_role() {
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let response = router(default_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/incidents")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "viewer")
                .body(Body::from(
                    serde_json::json!({
                        "id": incident.id, "tenant_id": incident.tenant_id, "title": incident.title,
                        "summary": incident.summary, "severity": "high", "status": "open",
                        "created_at": incident.created_at, "updated_at": incident.updated_at,
                        "resolved_at": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_incident_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "GET",
        format!("/v1/incidents/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_incidents_returns_backend_error_as_500() {
    let state = IncidentState {
        incident_repository: Arc::new(FailingIncidentRepository),
        audit_log_reader: Arc::new(InMemoryAuditLogReader::default()),
    };
    let response =
        send(router(state), "GET", "/v1/incidents".to_string(), Some(Uuid::new_v4()), None).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn full_incident_crud_and_link_round_trip() {
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let event_id = Uuid::new_v4();
    let state = default_state();

    let create = send(
        router(state.clone()),
        "POST",
        "/v1/incidents".to_string(),
        Some(tenant_id),
        Some(serde_json::json!({
            "id": incident.id, "tenant_id": incident.tenant_id, "title": incident.title,
            "summary": incident.summary, "severity": "high", "status": "open",
            "created_at": incident.created_at, "updated_at": incident.updated_at,
            "resolved_at": null
        })),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);

    let link = send(
        router(state.clone()),
        "POST",
        format!("/v1/incidents/{}/events", incident.id),
        Some(tenant_id),
        Some(serde_json::json!({ "event_id": event_id })),
    )
    .await;
    assert_eq!(link.status(), StatusCode::NO_CONTENT);

    let get = send(
        router(state.clone()),
        "GET",
        format!("/v1/incidents/{}", incident.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(get.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(get.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["event_ids"], serde_json::json!([event_id]));

    let mut updated = incident.clone();
    updated.status = IncidentStatus::Resolved;
    let update = send(
        router(state.clone()),
        "PUT",
        format!("/v1/incidents/{}", incident.id),
        Some(tenant_id),
        Some(serde_json::json!({
            "id": updated.id, "tenant_id": updated.tenant_id, "title": updated.title,
            "summary": updated.summary, "severity": "high", "status": "resolved",
            "created_at": updated.created_at, "updated_at": updated.updated_at,
            "resolved_at": null
        })),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);

    let unlink = send(
        router(state),
        "DELETE",
        format!("/v1/incidents/{}/events/{}", incident.id, event_id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(unlink.status(), StatusCode::NO_CONTENT);
}
