#[cfg(test)]
#[path = "api_test.rs"]
mod api_test;

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use common::ontology::{ActionInvocation, Link, LinkType, Object, ObjectType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::repository::OntologyRepository;

#[derive(Clone)]
pub struct ApiState {
    pub repository: Arc<dyn OntologyRepository>,
}

pub fn ontology_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/ontology/objects/types", get(list_object_types))
        .route("/api/ontology/objects/types/:id", get(get_object_type))
        .route("/api/ontology/links/types", get(list_link_types))
        .route("/api/ontology/objects", get(list_objects))
        .route("/api/ontology/objects/:id", get(get_object))
        .route("/api/ontology/objects/:id/links/:link_type_id", get(traverse_links))
        .route("/api/ontology/actions/invocations", get(list_action_invocations))
        .with_state(state)
}

// Implement handlers
async fn list_object_types(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<ObjectType>>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let types = state.repository.get_object_types(tenant_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(types))
}

async fn get_object_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ObjectType>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let obj_type = state.repository.get_object_type(tenant_id, id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(obj_type))
}

async fn list_link_types(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<LinkType>>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let types = state.repository.list_link_types(tenant_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(types))
}

#[derive(Deserialize)]
pub struct ObjectQuery {
    object_type_id: Option<Uuid>,
}

async fn list_objects(
    State(state): State<ApiState>,
    Query(query): Query<ObjectQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<Object>>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let objects = state.repository.list_objects(tenant_id, query.object_type_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(objects))
}

async fn get_object(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Object>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let obj = state.repository.get_object(tenant_id, id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(obj))
}

#[derive(Serialize)]
struct TraversalResult {
    links: Vec<Link>,
    targets: Vec<Object>,
}

async fn traverse_links(
    State(state): State<ApiState>,
    Path((id, link_type_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<TraversalResult>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let (links, targets) = state.repository.traverse_links(tenant_id, id, link_type_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(TraversalResult { links, targets }))
}

async fn list_action_invocations(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<ActionInvocation>>, StatusCode> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let actions = state.repository.list_action_invocations(tenant_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(actions))
}
