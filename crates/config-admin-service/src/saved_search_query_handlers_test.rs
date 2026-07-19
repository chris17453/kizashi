use super::*;
use crate::saved_search_query_repository::saved_search_query_repository_test::{
    FailingSavedSearchQueryRepository, InMemorySavedSearchQueryRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::post;
use axum::Router;
use tower::ServiceExt;

fn router(state: SavedSearchQueryState) -> Router {
    Router::new()
        .route(
            "/v1/saved-search-queries",
            post(create_saved_search_query).get(list_saved_search_queries),
        )
        .route("/v1/saved-search-queries/:id", axum::routing::delete(delete_saved_search_query))
        .with_state(state)
}

fn sample_query(tenant_id: Uuid) -> SavedSearchQuery {
    SavedSearchQuery::new(tenant_id, "urgent tickets", serde_json::json!({"q": "urgent"}))
}

fn default_state() -> SavedSearchQueryState {
    SavedSearchQueryState {
        saved_search_query_repository: Arc::new(InMemorySavedSearchQueryRepository::default()),
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
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    let body = body.map(|b| Body::from(b.to_string())).unwrap_or(Body::empty());
    app.oneshot(req.body(body).unwrap()).await.unwrap()
}

#[tokio::test]
async fn create_succeeds_for_any_authenticated_tenant_member_no_role_required() {
    let tenant_id = Uuid::new_v4();
    let query = sample_query(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/saved-search-queries".to_string(),
        Some(tenant_id),
        Some(serde_json::to_value(&query).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_rejects_a_tenant_mismatch() {
    let tenant_id = Uuid::new_v4();
    let query = sample_query(tenant_id);
    let response = send(
        router(default_state()),
        "POST",
        "/v1/saved-search-queries".to_string(),
        Some(Uuid::new_v4()),
        Some(serde_json::to_value(&query).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_returns_only_the_callers_tenant() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    state.saved_search_query_repository.create(sample_query(tenant_id)).await.unwrap();
    state.saved_search_query_repository.create(sample_query(Uuid::new_v4())).await.unwrap();

    let response =
        send(router(state), "GET", "/v1/saved-search-queries".to_string(), Some(tenant_id), None)
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn list_returns_500_on_backend_failure() {
    let state = SavedSearchQueryState {
        saved_search_query_repository: Arc::new(FailingSavedSearchQueryRepository),
    };
    let response = send(
        router(state),
        "GET",
        "/v1/saved-search-queries".to_string(),
        Some(Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn delete_removes_the_query() {
    let tenant_id = Uuid::new_v4();
    let state = default_state();
    let query = sample_query(tenant_id);
    state.saved_search_query_repository.create(query.clone()).await.unwrap();

    let response = send(
        router(state),
        "DELETE",
        format!("/v1/saved-search-queries/{}", query.id),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_for_an_unknown_id_returns_404() {
    let tenant_id = Uuid::new_v4();
    let response = send(
        router(default_state()),
        "DELETE",
        format!("/v1/saved-search-queries/{}", Uuid::new_v4()),
        Some(tenant_id),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
