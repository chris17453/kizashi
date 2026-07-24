use crate::api::{ontology_router, ApiState};
use crate::in_memory_repository::InMemoryOntologyRepository;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use common::ontology::ObjectType;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot` and `ready`
use uuid::Uuid;

#[test]
fn action_parameters_require_declared_fields_and_types() {
    let schema = json!({"reason":{"type":"string"},"notify":{"type":"boolean","required":false}});
    assert!(super::validate_action_parameters(&schema, &json!({"reason":"escalate"})).is_ok());
    assert_eq!(
        super::validate_action_parameters(&schema, &json!({})),
        Err(StatusCode::BAD_REQUEST)
    );
    assert_eq!(
        super::validate_action_parameters(&schema, &json!({"reason":42})),
        Err(StatusCode::BAD_REQUEST)
    );
    assert!(super::validate_action_parameters(
        &schema,
        &json!({"reason":"escalate","notify":true})
    )
    .is_ok());
}

#[test]
fn final_action_review_transitions_are_admin_only() {
    let mut operator = axum::http::HeaderMap::new();
    operator.insert("x-role", "operator".parse().unwrap());
    let mut admin = axum::http::HeaderMap::new();
    admin.insert("x-role", "admin".parse().unwrap());
    assert!(!super::can_approve_action_review(&operator));
    assert!(super::can_approve_action_review(&admin));
}

#[test]
fn parameterized_effects_resolve_without_treating_regular_json_as_a_template() {
    let parameters = serde_json::json!({"next_status": "Resolved", "notify": true});
    let effect = serde_json::json!({
        "status": {"$parameter": "next_status"},
        "metadata": {"source": "operator", "notify": {"$parameter": "notify"}},
        "labels": [{"$parameter": "next_status"}]
    });
    let resolved = super::resolve_effect_value(&effect, &parameters).unwrap();
    assert_eq!(resolved["status"], "Resolved");
    assert_eq!(resolved["metadata"]["notify"], true);
    assert_eq!(resolved["metadata"]["source"], "operator");
    assert_eq!(resolved["labels"][0], "Resolved");
}

#[test]
fn parameterized_effects_reject_missing_parameter_bindings() {
    let result = super::resolve_effect_value(
        &serde_json::json!({"status": {"$parameter": "missing"}}),
        &serde_json::json!({}),
    );
    assert_eq!(result, Err(axum::http::StatusCode::CONFLICT));
}

#[test]
fn object_properties_follow_the_declared_type_contract() {
    let schema = json!({
        "status": {"type":"string", "required":true},
        "attempts": {"type":"integer"},
        "active": {"type":"boolean"}
    });
    assert!(super::validate_object_properties(&schema, &json!({"status":"open"})).is_ok());
    assert!(super::validate_object_properties(
        &schema,
        &json!({"status":"open", "attempts":2, "active":true, "vendor_field":"kept"})
    )
    .is_ok());
    assert_eq!(
        super::validate_object_properties(&schema, &json!({})),
        Err(StatusCode::BAD_REQUEST)
    );
    assert_eq!(
        super::validate_object_properties(&schema, &json!({"status":42})),
        Err(StatusCode::BAD_REQUEST)
    );
    assert_eq!(
        super::validate_object_properties(&schema, &json!({"status":"open", "attempts":1.5})),
        Err(StatusCode::BAD_REQUEST)
    );
}

#[tokio::test]
async fn test_get_object_types() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let state = ApiState { repository: repo.clone() };

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

#[tokio::test]
async fn object_mutations_are_exposed_as_immutable_history() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let state = ApiState { repository: repo.clone() };
    let tenant_id = Uuid::new_v4();
    let object_type_id = Uuid::new_v4();
    repo.create_object_type(ObjectType {
        id: object_type_id,
        tenant_id,
        name: "Customer".to_string(),
        version: 1,
        property_schema: json!({}),
        mapping_rules: json!([]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .await
    .unwrap();

    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/objects")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "alice")
                .body(Body::from(
                    json!({"object_type_id": object_type_id, "properties":{"name":"Northwind"}, "source_lineage":[]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let object: common::ontology::Object = serde_json::from_slice(&body).unwrap();

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/ontology/objects/{}/history", object.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::ObjectHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].change_type, "created");
    assert_eq!(history[0].actor, "alice");
    assert_eq!(history[0].after_state.as_ref().unwrap()["properties"]["name"], "Northwind");
}
