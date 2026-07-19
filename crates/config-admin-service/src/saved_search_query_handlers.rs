#[path = "saved_search_query_handlers_test.rs"]
#[cfg(test)]
mod saved_search_query_handlers_test;

use crate::handlers::{tenant_id_from_headers, tenant_mismatch};
use crate::saved_search_query_repository::{
    SavedSearchQueryRepository, SavedSearchQueryRepositoryError,
};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use common::SavedSearchQuery;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct SavedSearchQueryState {
    pub saved_search_query_repository: Arc<dyn SavedSearchQueryRepository>,
}

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: message.into() })).into_response()
}

fn saved_search_query_error_response(e: SavedSearchQueryRepositoryError) -> Response {
    match e {
        SavedSearchQueryRepositoryError::NotFound(id) => {
            error_response(StatusCode::NOT_FOUND, format!("no saved search query with id {id}"))
        }
        SavedSearchQueryRepositoryError::Backend(msg) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

/// POST /v1/saved-search-queries — no `require_operator` gate (ADR-0029): a saved search is a
/// personal/team bookmark, not admin/config that changes platform behavior, so any
/// authenticated tenant member (including `Viewer`) can save/list/delete one.
pub async fn create_saved_search_query(
    State(state): State<SavedSearchQueryState>,
    headers: HeaderMap,
    Json(query): Json<SavedSearchQuery>,
) -> Response {
    if let Some(response) = tenant_mismatch(&headers, query.tenant_id) {
        return response;
    }
    match state.saved_search_query_repository.create(query).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(e) => saved_search_query_error_response(e),
    }
}

pub async fn list_saved_search_queries(
    State(state): State<SavedSearchQueryState>,
    headers: HeaderMap,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.saved_search_query_repository.list(tenant_id).await {
        Ok(queries) => Json(queries).into_response(),
        Err(e) => saved_search_query_error_response(e),
    }
}

pub async fn delete_saved_search_query(
    State(state): State<SavedSearchQueryState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(id) => id,
        Err((status, msg)) => return error_response(status, msg),
    };
    match state.saved_search_query_repository.delete(tenant_id, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => saved_search_query_error_response(e),
    }
}
