#[path = "ontology_object_360_handler_test.rs"]
#[cfg(test)]
mod ontology_object_360_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

#[derive(Debug, Template)]
#[template(path = "ontology_object_360.html")]
pub struct OntologyObject360Template {
    pub show_nav: bool,
    pub is_admin: bool,
    pub can_manage: bool,
    pub object_id: Uuid,
}

/// Renders the dedicated object investigation workspace. The bounded, tenant-scoped read model
/// is served by the versioned endpoint and progressively hydrates this authenticated shell so
/// external API consumers and browser operators use the same object-360 contract.
pub async fn get_ontology_object_360(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(object_id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    Html(
        OntologyObject360Template {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            can_manage: session.role.at_least(common::Role::Operator),
            object_id,
        }
        .render()
        .unwrap_or_else(|_| "Unable to render object workspace".to_string()),
    )
    .into_response()
}
