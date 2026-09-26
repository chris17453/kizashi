use super::{ontology_router, ApiState};
use crate::in_memory_repository::InMemoryOntologyRepository;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn object_type_api_rejects_invalid_rich_property_definitions() {
    let tenant_id = Uuid::new_v4();
    let request = Request::builder()
        .method("POST")
        .uri("/api/ontology/objects/types")
        .header("content-type", "application/json")
        .header("x-tenant-id", tenant_id.to_string())
        .header("x-role", "operator")
        .body(Body::from(
            json!({
                "name": "Account",
                "version": 1,
                "property_schema": {"state": {"type": "enum"}},
                "mapping_rules": []
            })
            .to_string(),
        ))
        .unwrap();

    let response =
        ontology_router(ApiState { repository: Arc::new(InMemoryOntologyRepository::new()) })
            .oneshot(request)
            .await
            .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
