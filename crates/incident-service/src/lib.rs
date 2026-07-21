mod audit_log;
mod handlers;
mod health;
mod incident_repository;

use axum::routing::{get, post};
use axum::Router;

pub use audit_log::{
    AuditLogEntry, AuditLogError, AuditLogReader, ChangeType, PostgresAuditLogReader,
};
pub use handlers::{
    create_incident, get_incident, link_event, list_incidents, unlink_event, update_incident,
    IncidentState,
};
pub use incident_repository::{
    IncidentRepository, IncidentRepositoryError, PostgresIncidentRepository,
};

pub fn build_router(state: IncidentState) -> Router {
    let incident_routes = Router::new()
        .route("/v1/incidents", post(create_incident).get(list_incidents))
        .route("/v1/incidents/:id", get(get_incident).put(update_incident))
        .route("/v1/incidents/:id/events", post(link_event))
        .route("/v1/incidents/:id/events/:event_id", axum::routing::delete(unlink_event))
        .with_state(state);

    incident_routes.merge(health::build_router())
}
