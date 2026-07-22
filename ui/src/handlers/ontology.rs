use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use common::ontology::{ActionType, ObjectType};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Template)]
#[template(path = "ontology.html")]
struct OntologyTemplate {
    object_types: Vec<ObjectType>,
    action_types: Vec<ActionType>,
}

#[derive(Deserialize)]
pub struct OntologyQuery {
    tenant_id: Option<uuid::Uuid>,
}

pub async fn list_ontology(
    State(state): State<AppState>,
    Query(query): Query<OntologyQuery>,
) -> impl IntoResponse {
    let tenant_id = query.tenant_id.unwrap_or_else(|| uuid::Uuid::nil());
    
    // In a real implementation this would fetch from query-gateway
    let object_types = Vec::new();
    let action_types = Vec::new();

    let template = OntologyTemplate {
        object_types,
        action_types,
    };
    Html(template.render().unwrap())
}
