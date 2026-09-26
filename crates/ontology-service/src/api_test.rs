use crate::api::{ontology_router, ApiState};
use crate::in_memory_repository::InMemoryOntologyRepository;
use crate::repository::OntologyRepository;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use common::ontology::{Link, ObjectType};
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
    let link_schema = json!({"role":{"type":"string","required":true}});
    assert!(super::validate_object_properties(&link_schema, &json!({"role":"owner"})).is_ok());
    assert_eq!(
        super::validate_object_properties(&link_schema, &json!({})),
        Err(StatusCode::BAD_REQUEST)
    );
}

#[test]
fn relationship_cardinality_rejects_only_the_constrained_endpoint() {
    let source = Uuid::new_v4();
    let target = Uuid::new_v4();
    let link = Link {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        link_type_id: Uuid::new_v4(),
        source_object_id: source,
        target_object_id: target,
        properties: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(super::link_cardinality_violated(
        "many-to-one",
        std::slice::from_ref(&link),
        source,
        Uuid::new_v4(),
        None
    )
    .unwrap());
    assert!(!super::link_cardinality_violated(
        "many-to-one",
        std::slice::from_ref(&link),
        Uuid::new_v4(),
        target,
        None
    )
    .unwrap());
    assert!(super::link_cardinality_violated(
        "one-to-many",
        std::slice::from_ref(&link),
        Uuid::new_v4(),
        target,
        None
    )
    .unwrap());
    assert!(super::link_cardinality_violated(
        "one-to-one",
        std::slice::from_ref(&link),
        Uuid::new_v4(),
        target,
        None
    )
    .unwrap());
    assert!(!super::link_cardinality_violated(
        "one-to-one",
        std::slice::from_ref(&link),
        source,
        target,
        Some(link.id)
    )
    .unwrap());
    assert_eq!(
        super::link_cardinality_violated(
            "invalid",
            std::slice::from_ref(&link),
            source,
            target,
            None
        ),
        Err(StatusCode::BAD_REQUEST)
    );
}

#[tokio::test]
async fn object_annotations_are_tenant_scoped_and_append_only() {
    let repo = InMemoryOntologyRepository::new();
    let tenant_id = Uuid::new_v4();
    let object_id = Uuid::new_v4();
    repo.create_object(common::ontology::Object {
        id: object_id,
        tenant_id,
        object_type_id: Uuid::new_v4(),
        properties: json!({"name":"Annotated object"}),
        source_lineage: json!([]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .await
    .unwrap();
    let state = ApiState { repository: Arc::new(repo) };
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/ontology/objects/{object_id}/annotations"))
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "analyst")
                .body(Body::from(
                    json!({"body":"Observed a repeated escalation pattern."}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/ontology/objects/{object_id}/annotations"))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let annotations: Vec<common::ontology::ObjectAnnotation> =
        serde_json::from_slice(&body).unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].author, "analyst");
    assert_eq!(annotations[0].body, "Observed a repeated escalation pattern.");

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/ontology/objects/annotations")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let all_annotations: Vec<common::ontology::ObjectAnnotation> =
        serde_json::from_slice(&body).unwrap();
    assert_eq!(all_annotations.len(), 1);
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

#[tokio::test]
async fn global_object_type_history_retains_deleted_contracts() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let headers = |request: axum::http::request::Builder, role: &str| {
        request
            .header("x-tenant-id", tenant_id.to_string())
            .header("x-role", role)
            .header("x-username", "modeler")
    };
    let response = ontology_router(ApiState { repository: repo.clone() })
        .oneshot(
            headers(
                Request::builder()
                    .method("POST")
                    .uri("/api/ontology/objects/types")
                    .header("content-type", "application/json"),
                "operator",
            )
            .body(Body::from(
                json!({"name":"Customer","version":1,"property_schema":{},"mapping_rules":{}})
                    .to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let object_type: common::ontology::ObjectType = serde_json::from_slice(&body).unwrap();
    let response = ontology_router(ApiState { repository: repo.clone() })
        .oneshot(
            headers(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/ontology/objects/types/{}", object_type.id)),
                "admin",
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = ontology_router(ApiState { repository: repo })
        .oneshot(
            Request::builder()
                .uri("/api/ontology/objects/types/history")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::ObjectTypeHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].change_type, "deleted");
    assert_eq!(history[1].change_type, "created");
}

#[tokio::test]
async fn global_object_history_retains_deleted_entities() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let object_id = Uuid::new_v4();
    repo.record_object_history(common::ontology::ObjectHistory {
        id: Uuid::new_v4(),
        tenant_id,
        object_id,
        change_type: "created".to_string(),
        actor: "modeler".to_string(),
        before_state: None,
        after_state: Some(json!({"id": object_id, "properties": {"name": "Northwind"}})),
        changed_at: Utc::now(),
    })
    .await
    .unwrap();
    repo.record_object_history(common::ontology::ObjectHistory {
        id: Uuid::new_v4(),
        tenant_id,
        object_id,
        change_type: "deleted".to_string(),
        actor: "admin".to_string(),
        before_state: Some(json!({"id": object_id, "properties": {"name": "Northwind"}})),
        after_state: None,
        changed_at: Utc::now(),
    })
    .await
    .unwrap();
    let response = ontology_router(ApiState { repository: repo })
        .oneshot(
            Request::builder()
                .uri("/api/ontology/objects/history")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::ObjectHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].change_type, "deleted");
}

#[tokio::test]
async fn global_link_history_retains_deleted_edges() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let link_id = Uuid::new_v4();
    repo.record_link_history(common::ontology::LinkHistory {
        id: Uuid::new_v4(), tenant_id, link_id, change_type: "created".to_string(),
        actor: "modeler".to_string(), before_state: None, after_state: Some(json!({"id": link_id, "source_object_id": Uuid::new_v4(), "target_object_id": Uuid::new_v4()})), changed_at: Utc::now(),
    }).await.unwrap();
    repo.record_link_history(common::ontology::LinkHistory {
        id: Uuid::new_v4(),
        tenant_id,
        link_id,
        change_type: "deleted".to_string(),
        actor: "admin".to_string(),
        before_state: Some(json!({"id": link_id})),
        after_state: None,
        changed_at: Utc::now(),
    })
    .await
    .unwrap();
    let response = ontology_router(ApiState { repository: repo })
        .oneshot(
            Request::builder()
                .uri("/api/ontology/links/history")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::LinkHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].change_type, "deleted");
}

#[tokio::test]
async fn action_type_history_uses_the_forwarded_actor_identity() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let state = ApiState { repository: repo };
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/actions/types")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-actor", "operator")
                .body(Body::from(
                    json!({
                        "name": "Escalate account",
                        "target_object_type_id": null,
                        "parameter_schema": {},
                        "preconditions": {},
                        "effect_definition": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let action: common::ontology::ActionType = serde_json::from_slice(&body).unwrap();

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/ontology/actions/types/{}/history", action.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::ActionTypeHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].actor, "operator");
}

#[tokio::test]
async fn contract_updates_preserve_creation_timestamps() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let source_type_id = Uuid::new_v4();
    let target_type_id = Uuid::new_v4();
    for (id, name) in [(source_type_id, "Source"), (target_type_id, "Target")] {
        repo.create_object_type(ObjectType {
            id,
            tenant_id,
            name: name.to_string(),
            version: 1,
            property_schema: json!({}),
            mapping_rules: json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .unwrap();
    }
    let state = ApiState { repository: repo };
    let action_response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/actions/types")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::from(
                    json!({"name":"Notify","parameter_schema":{},"preconditions":{},"effect_definition":{}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(action_response.into_body(), usize::MAX).await.unwrap();
    let action: common::ontology::ActionType = serde_json::from_slice(&body).unwrap();
    let action_created_at = action.created_at;

    let link_response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/links/types")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::from(
                    json!({"name":"Owns","source_object_type_id":source_type_id,"target_object_type_id":target_type_id,"cardinality":"many-to-one"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(link_response.into_body(), usize::MAX).await.unwrap();
    let link: common::ontology::LinkType = serde_json::from_slice(&body).unwrap();
    let link_created_at = link.created_at;

    for (uri, payload) in [
        (
            format!("/api/ontology/actions/types/{}", action.id),
            json!({"name":"Notify updated","parameter_schema":{},"preconditions":{},"effect_definition":{}}),
        ),
        (
            format!("/api/ontology/links/types/{}", link.id),
            json!({"name":"Owns updated","source_object_type_id":source_type_id,"target_object_type_id":target_type_id,"cardinality":"one-to-many"}),
        ),
    ] {
        let response = ontology_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .header("x-tenant-id", tenant_id.to_string())
                    .header("x-role", "operator")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
    }

    let actions = state.repository.list_action_types(tenant_id).await.unwrap();
    let links = state.repository.list_link_types(tenant_id).await.unwrap();
    assert_eq!(actions[0].created_at, action_created_at);
    assert_eq!(links[0].created_at, link_created_at);
}

#[tokio::test]
async fn global_action_history_retains_deleted_contracts() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let state = ApiState { repository: repo };
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/actions/types")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .body(Body::from(
                    json!({"name":"Delete me","parameter_schema":{},"preconditions":{},"effect_definition":{}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let action: common::ontology::ActionType = serde_json::from_slice(&body).unwrap();
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/ontology/actions/types/{}", action.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/ontology/actions/types/history")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::ActionTypeHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].change_type, "deleted");
    assert_eq!(history[1].change_type, "created");
}

#[tokio::test]
async fn object_type_mutations_are_exposed_as_version_history() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let state = ApiState { repository: repo };
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/objects/types")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("x-username", "modeler")
                .body(Body::from(
                    json!({"name":"Customer","version":1,"property_schema":{},"mapping_rules":{}})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let object_type: ObjectType = serde_json::from_slice(&body).unwrap();

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/ontology/objects/types/{}/history", object_type.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::ObjectTypeHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].change_type, "created");
    assert_eq!(history[0].actor, "modeler");
}

#[tokio::test]
async fn link_type_mutations_are_exposed_as_version_history() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let state = ApiState { repository: repo };
    let source_type = Uuid::new_v4();
    let target_type = Uuid::new_v4();
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/links/types")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "modeler")
                .body(Body::from(
                    json!({"name":"Raised by","source_object_type_id":source_type,"target_object_type_id":target_type,"cardinality":"many-to-one","properties_schema":{}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let link_type: common::ontology::LinkType = serde_json::from_slice(&body).unwrap();

    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/ontology/links/types/{}", link_type.id))
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "modeler")
                .body(Body::from(
                    json!({"name":"Owned by","source_object_type_id":source_type,"target_object_type_id":target_type,"cardinality":"one-to-one","properties_schema":{}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/ontology/links/types/{}", link_type.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("x-username", "admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/ontology/links/types/{}/history", link_type.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::LinkTypeHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].change_type, "deleted");
    assert_eq!(history[1].change_type, "updated");
    assert_eq!(history[2].change_type, "created");
    assert_eq!(history[1].before_state.as_ref().unwrap()["cardinality"], "many-to-one");
    assert_eq!(history[1].after_state.as_ref().unwrap()["cardinality"], "one-to-one");
}

#[tokio::test]
async fn global_link_type_history_retains_deleted_contracts() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let source_type = Uuid::new_v4();
    let target_type = Uuid::new_v4();
    let request = Request::builder()
        .method("POST")
        .uri("/api/ontology/links/types")
        .header("content-type", "application/json")
        .header("x-tenant-id", tenant_id.to_string())
        .header("x-role", "operator")
        .header("x-username", "modeler")
        .body(Body::from(json!({"name":"Raised by","source_object_type_id":source_type,"target_object_type_id":target_type,"cardinality":"many-to-one","properties_schema":{}}).to_string()))
        .unwrap();
    let response =
        ontology_router(ApiState { repository: repo.clone() }).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let link_type: common::ontology::LinkType = serde_json::from_slice(&body).unwrap();
    let response = ontology_router(ApiState { repository: repo.clone() })
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/ontology/links/types/{}", link_type.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("x-username", "operator")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = ontology_router(ApiState { repository: repo })
        .oneshot(
            Request::builder()
                .uri("/api/ontology/links/types/history")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::LinkTypeHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].change_type, "deleted");
    assert_eq!(history[1].change_type, "created");
}

#[tokio::test]
async fn link_instance_mutations_are_exposed_as_version_history() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let tenant_id = Uuid::new_v4();
    let source_type = Uuid::new_v4();
    let target_type = Uuid::new_v4();
    let link_type_id = Uuid::new_v4();
    let source_id = Uuid::new_v4();
    let target_id = Uuid::new_v4();
    repo.create_link_type(common::ontology::LinkType {
        id: link_type_id,
        tenant_id,
        name: "Raised by".to_string(),
        source_object_type_id: source_type,
        target_object_type_id: target_type,
        cardinality: "many-to-one".to_string(),
        properties_schema: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .await
    .unwrap();
    for (id, object_type_id) in [(source_id, source_type), (target_id, target_type)] {
        repo.create_object(common::ontology::Object {
            id,
            tenant_id,
            object_type_id,
            properties: json!({"name": id.to_string()}),
            source_lineage: json!([]),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .unwrap();
    }
    let state = ApiState { repository: repo };
    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ontology/links")
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "operator")
                .body(Body::from(
                    json!({"link_type_id":link_type_id,"source_object_id":source_id,"target_object_id":target_id,"properties":{"confidence":0.7}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let link: common::ontology::Link = serde_json::from_slice(&body).unwrap();

    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/ontology/links/{}", link.id))
                .header("content-type", "application/json")
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "operator")
                .header("x-username", "operator")
                .body(Body::from(
                    json!({"link_type_id":link_type_id,"source_object_id":source_id,"target_object_id":target_id,"properties":{"confidence":0.95}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = ontology_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/ontology/links/{}", link.id))
                .header("x-tenant-id", tenant_id.to_string())
                .header("x-role", "admin")
                .header("x-username", "admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = ontology_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/ontology/links/{}/history", link.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let history: Vec<common::ontology::LinkHistory> = serde_json::from_slice(&body).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].change_type, "deleted");
    assert_eq!(history[1].change_type, "updated");
    assert_eq!(history[2].change_type, "created");
    assert_eq!(history[1].before_state.as_ref().unwrap()["properties"]["confidence"], 0.7);
    assert_eq!(history[1].after_state.as_ref().unwrap()["properties"]["confidence"], 0.95);
}
