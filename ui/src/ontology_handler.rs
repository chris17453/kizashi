use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use common::ontology::{ActionType, ObjectType};
use serde::Deserialize;

use crate::AppState;

#[derive(Template)]
#[template(path = "ontology.html")]
struct OntologyTemplate {
    show_nav: bool,
    is_admin: bool,
    object_types: Vec<ObjectType>,
    action_types: Vec<ActionType>,
}

#[derive(Deserialize)]
pub struct OntologyQuery {
    pub tenant_id: Option<uuid::Uuid>,
}

pub async fn list_ontology(
    State(_state): State<AppState>,
    Query(_query): Query<OntologyQuery>,
) -> impl IntoResponse {
    // In a real implementation this would fetch from query-gateway
    let object_types = Vec::new();
    let action_types = Vec::new();

    let template = OntologyTemplate {
        show_nav: true,
        is_admin: true,
        object_types,
        action_types,
    };
    Html(template.render().unwrap())
}
