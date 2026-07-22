use crate::api::{ApiState, ontology_router};
use crate::in_memory_repository::InMemoryOntologyRepository;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::ontology::ObjectType;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot` and `ready`
use uuid::Uuid;
use chrono::Utc;

#[tokio::test]
async fn test_get_object_types() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let state = ApiState {
        repository: repo.clone(),
    };

    let tenant_id = Uuid::new_v4();
    repo.create_object_type(ObjectType {
        id: Uuid::new_v4(),
        tenant_id,
        name: "TestType".to_string(),
        version: 1,
        property_schema: json!({}),
        mapping_rules: json!([]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .await
    .unwrap();

    let app = ontology_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/ontology/objects/types") // The router prefix is applied in main.rs typically, or inside the router. Let's check router definition
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
