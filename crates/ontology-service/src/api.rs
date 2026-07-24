#[cfg(test)]
#[path = "api_test.rs"]
mod api_test;

use crate::repository::OntologyRepository;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use common::ontology::{
    ActionInvocation, ActionReview, ActionType, ActionTypeHistory, Link, LinkType, Object,
    ObjectHistory, ObjectType,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    pub repository: Arc<dyn OntologyRepository>,
}

pub fn ontology_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/ontology/objects/types", get(list_object_types))
        .route("/api/ontology/objects/types", post(create_object_type))
        .route("/api/ontology/objects/types/:id", get(get_object_type))
        .route(
            "/api/ontology/objects/types/:id",
            post(update_object_type).delete(delete_object_type),
        )
        .route("/api/ontology/links/types", get(list_link_types))
        .route("/api/ontology/links/types", post(create_link_type))
        .route("/api/ontology/links/types/:id", post(update_link_type).delete(delete_link_type))
        .route("/api/ontology/links", get(list_links).post(create_link))
        .route("/api/ontology/links/:id", post(update_link).delete(delete_link))
        .route("/api/ontology/objects", get(list_objects))
        .route("/api/ontology/objects", post(create_object))
        .route("/api/ontology/objects/:id", get(get_object))
        .route("/api/ontology/objects/:id", post(update_object).delete(delete_object))
        .route("/api/ontology/objects/:id/history", get(list_object_history))
        .route("/api/ontology/objects/:id/links/:link_type_id", get(traverse_links))
        .route("/api/ontology/actions/invocations", get(list_action_invocations))
        .route("/api/ontology/actions/reviews", get(list_action_reviews).post(upsert_action_review))
        .route("/api/ontology/actions/invoke", post(invoke_action))
        .route("/api/ontology/actions/types", get(list_action_types).post(create_action_type))
        .route("/api/ontology/actions/types/:id/history", get(list_action_type_history))
        .route(
            "/api/ontology/actions/types/:id",
            post(update_action_type).delete(delete_action_type),
        )
        .with_state(state)
}

fn tenant(headers: &HeaderMap) -> Result<Uuid, StatusCode> {
    headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(StatusCode::UNAUTHORIZED)
}

fn can_write(headers: &HeaderMap) -> bool {
    matches!(headers.get("x-role").and_then(|h| h.to_str().ok()), Some("admin" | "operator"))
}

fn can_approve_action_review(headers: &HeaderMap) -> bool {
    matches!(headers.get("x-role").and_then(|h| h.to_str().ok()), Some("admin"))
}

fn write_check(headers: &HeaderMap) -> Result<Uuid, StatusCode> {
    if !can_write(headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    tenant(headers)
}

fn actor(headers: &HeaderMap) -> String {
    headers
        .get("x-username")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("system")
        .to_string()
}

fn object_state(object: &Object) -> serde_json::Value {
    serde_json::json!({
        "object_type_id": object.object_type_id,
        "properties": object.properties,
        "source_lineage": object.source_lineage,
    })
}

#[derive(Deserialize)]
struct ObjectTypeInput {
    name: String,
    version: i32,
    property_schema: serde_json::Value,
    mapping_rules: serde_json::Value,
}

#[derive(Deserialize)]
struct ObjectInput {
    object_type_id: Uuid,
    properties: serde_json::Value,
    source_lineage: serde_json::Value,
}

async fn create_object(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(input): Json<ObjectInput>,
) -> Result<Json<Object>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let object_type = state
        .repository
        .get_object_type(tenant_id, input.object_type_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    validate_object_properties(&object_type.property_schema, &input.properties)?;
    let now = chrono::Utc::now();
    let object = Object {
        id: Uuid::new_v4(),
        tenant_id,
        object_type_id: input.object_type_id,
        properties: input.properties,
        source_lineage: input.source_lineage,
        created_at: now,
        updated_at: now,
    };
    state.repository.create_object(object.clone()).await.map_err(|_| StatusCode::CONFLICT)?;
    state
        .repository
        .record_object_history(ObjectHistory {
            id: Uuid::new_v4(),
            tenant_id,
            object_id: object.id,
            change_type: "created".to_string(),
            actor: actor(&headers),
            before_state: None,
            after_state: Some(object_state(&object)),
            changed_at: object.created_at,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(object))
}

async fn update_object(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ObjectInput>,
) -> Result<Json<Object>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let existing = state
        .repository
        .get_object(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let object_type = state
        .repository
        .get_object_type(tenant_id, input.object_type_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    validate_object_properties(&object_type.property_schema, &input.properties)?;
    let object = Object {
        id,
        tenant_id,
        object_type_id: input.object_type_id,
        properties: input.properties,
        source_lineage: input.source_lineage,
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
    };
    state
        .repository
        .update_object(object.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .repository
        .record_object_history(ObjectHistory {
            id: Uuid::new_v4(),
            tenant_id,
            object_id: object.id,
            change_type: "updated".to_string(),
            actor: actor(&headers),
            before_state: Some(object_state(&existing)),
            after_state: Some(object_state(&object)),
            changed_at: object.updated_at,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(object))
}

async fn delete_object(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let existing = state
        .repository
        .get_object(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    state.repository.delete_object(tenant_id, id).await.map_err(|_| StatusCode::CONFLICT)?;
    state
        .repository
        .record_object_history(ObjectHistory {
            id: Uuid::new_v4(),
            tenant_id,
            object_id: id,
            change_type: "deleted".to_string(),
            actor: actor(&headers),
            before_state: Some(object_state(&existing)),
            after_state: None,
            changed_at: chrono::Utc::now(),
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_object_history(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Vec<ObjectHistory>>, StatusCode> {
    let tenant_id = tenant(&headers)?;
    state
        .repository
        .list_object_history(tenant_id, id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_object_type(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(input): Json<ObjectTypeInput>,
) -> Result<Json<ObjectType>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let now = chrono::Utc::now();
    let value = ObjectType {
        id: Uuid::new_v4(),
        tenant_id,
        name: input.name,
        version: input.version,
        property_schema: input.property_schema,
        mapping_rules: input.mapping_rules,
        created_at: now,
        updated_at: now,
    };
    state.repository.create_object_type(value.clone()).await.map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(value))
}

async fn update_object_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ObjectTypeInput>,
) -> Result<Json<ObjectType>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let existing = state
        .repository
        .get_object_type(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let value = ObjectType {
        id,
        tenant_id,
        name: input.name,
        version: input.version,
        property_schema: input.property_schema,
        mapping_rules: input.mapping_rules,
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
    };
    state
        .repository
        .update_object_type(value.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(value))
}

async fn delete_object_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = write_check(&headers)?;
    state.repository.delete_object_type(tenant_id, id).await.map_err(|_| StatusCode::CONFLICT)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct LinkTypeInput {
    name: String,
    source_object_type_id: Uuid,
    target_object_type_id: Uuid,
    cardinality: String,
    properties_schema: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct LinkInput {
    link_type_id: Uuid,
    source_object_id: Uuid,
    target_object_id: Uuid,
    properties: Option<serde_json::Value>,
}

async fn list_links(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Link>>, StatusCode> {
    Ok(Json(
        state
            .repository
            .list_links(tenant(&headers)?)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

async fn validate_link_endpoints(
    state: &ApiState,
    tenant_id: Uuid,
    input: &LinkInput,
) -> Result<(), StatusCode> {
    let link_type = state
        .repository
        .list_link_types(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|link| link.id == input.link_type_id)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let source = state
        .repository
        .get_object(tenant_id, input.source_object_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    let target = state
        .repository
        .get_object(tenant_id, input.target_object_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    if source.object_type_id != link_type.source_object_type_id
        || target.object_type_id != link_type.target_object_type_id
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

async fn create_link(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(input): Json<LinkInput>,
) -> Result<Json<Link>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    validate_link_endpoints(&state, tenant_id, &input).await?;
    let now = chrono::Utc::now();
    let link = Link {
        id: Uuid::new_v4(),
        tenant_id,
        link_type_id: input.link_type_id,
        source_object_id: input.source_object_id,
        target_object_id: input.target_object_id,
        properties: input.properties,
        created_at: now,
        updated_at: now,
    };
    state.repository.create_link(link.clone()).await.map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(link))
}

async fn update_link(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<LinkInput>,
) -> Result<Json<Link>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    validate_link_endpoints(&state, tenant_id, &input).await?;
    let existing = state
        .repository
        .list_links(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|link| link.id == id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let link = Link {
        id,
        tenant_id,
        link_type_id: input.link_type_id,
        source_object_id: input.source_object_id,
        target_object_id: input.target_object_id,
        properties: input.properties,
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
    };
    state
        .repository
        .update_link(link.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(link))
}

async fn delete_link(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    state
        .repository
        .delete_link(write_check(&headers)?, id)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_link_type(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(input): Json<LinkTypeInput>,
) -> Result<Json<LinkType>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let now = chrono::Utc::now();
    let value = LinkType {
        id: Uuid::new_v4(),
        tenant_id,
        name: input.name,
        source_object_type_id: input.source_object_type_id,
        target_object_type_id: input.target_object_type_id,
        cardinality: input.cardinality,
        properties_schema: input.properties_schema,
        created_at: now,
        updated_at: now,
    };
    state.repository.create_link_type(value.clone()).await.map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(value))
}

async fn delete_link_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    state
        .repository
        .delete_link_type(write_check(&headers)?, id)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_link_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<LinkTypeInput>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let now = chrono::Utc::now();
    let value = LinkType {
        id,
        tenant_id,
        name: input.name,
        source_object_type_id: input.source_object_type_id,
        target_object_type_id: input.target_object_type_id,
        cardinality: input.cardinality,
        properties_schema: input.properties_schema,
        created_at: now,
        updated_at: now,
    };
    state
        .repository
        .update_link_type(value)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_action_types(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ActionType>>, StatusCode> {
    Ok(Json(
        state
            .repository
            .list_action_types(tenant(&headers)?)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

async fn list_action_type_history(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Vec<ActionTypeHistory>>, StatusCode> {
    Ok(Json(
        state
            .repository
            .list_action_type_history(tenant(&headers)?, id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

fn action_history(
    action: &ActionType,
    change_type: &str,
    actor: &str,
    before_state: Option<serde_json::Value>,
    after_state: Option<serde_json::Value>,
) -> ActionTypeHistory {
    ActionTypeHistory {
        id: Uuid::new_v4(),
        tenant_id: action.tenant_id,
        action_type_id: action.id,
        change_type: change_type.to_string(),
        actor: actor.to_string(),
        before_state,
        after_state,
        changed_at: chrono::Utc::now(),
    }
}

#[derive(Deserialize)]
struct ActionTypeInput {
    name: String,
    target_object_type_id: Option<Uuid>,
    parameter_schema: serde_json::Value,
    preconditions: serde_json::Value,
    effect_definition: serde_json::Value,
}

#[derive(Deserialize)]
struct InvokeActionInput {
    action_type_id: Uuid,
    target_object_ids: Vec<Uuid>,
    parameters: serde_json::Value,
    #[serde(default)]
    triggering_event_ref: Option<serde_json::Value>,
}

fn validate_action_parameters(
    schema: &serde_json::Value,
    parameters: &serde_json::Value,
) -> Result<(), StatusCode> {
    let Some(schema) = schema.as_object() else {
        return if schema.is_null() { Ok(()) } else { Err(StatusCode::BAD_REQUEST) };
    };
    let Some(parameters) = parameters.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    for (name, definition) in schema {
        let Some(definition) = definition.as_object() else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let required =
            definition.get("required").and_then(serde_json::Value::as_bool).unwrap_or(true);
        let Some(value) = parameters.get(name) else {
            if required {
                return Err(StatusCode::BAD_REQUEST);
            }
            continue;
        };
        let Some(expected) = definition.get("type").and_then(serde_json::Value::as_str) else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let matches = match expected {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => false,
        };
        if !matches {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

/// Resolves the small, explicit parameter-binding form supported by governed effects:
/// `{ "$parameter": "field_name" }` is replaced with the already schema-validated action
/// parameter. All other JSON is traversed recursively, so nested effect payloads remain useful
/// without turning the contract into an unsafe template language.
fn resolve_effect_value(
    effect: &serde_json::Value,
    parameters: &serde_json::Value,
) -> Result<serde_json::Value, StatusCode> {
    match effect {
        serde_json::Value::Object(object)
            if object.len() == 1 && object.contains_key("$parameter") =>
        {
            let Some(name) = object.get("$parameter").and_then(serde_json::Value::as_str) else {
                return Err(StatusCode::CONFLICT);
            };
            parameters.get(name).cloned().ok_or(StatusCode::CONFLICT)
        }
        serde_json::Value::Object(object) => object
            .iter()
            .map(|(key, value)| Ok((key.clone(), resolve_effect_value(value, parameters)?)))
            .collect::<Result<serde_json::Map<_, _>, StatusCode>>()
            .map(serde_json::Value::Object),
        serde_json::Value::Array(values) => values
            .iter()
            .map(|value| resolve_effect_value(value, parameters))
            .collect::<Result<Vec<_>, StatusCode>>()
            .map(serde_json::Value::Array),
        value => Ok(value.clone()),
    }
}

/// Object types are executable contracts, not decorative metadata. Validate declared fields and
/// their primitive JSON types at the write boundary while allowing additive properties so source
/// systems can carry fields introduced before a model version is updated.
fn validate_object_properties(
    schema: &serde_json::Value,
    properties: &serde_json::Value,
) -> Result<(), StatusCode> {
    let Some(schema) = schema.as_object() else {
        return if schema.is_null() { Ok(()) } else { Err(StatusCode::BAD_REQUEST) };
    };
    let Some(properties) = properties.as_object() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    for (name, definition) in schema {
        let Some(definition) = definition.as_object() else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let required =
            definition.get("required").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let Some(value) = properties.get(name) else {
            if required {
                return Err(StatusCode::BAD_REQUEST);
            }
            continue;
        };
        let Some(expected) = definition.get("type").and_then(serde_json::Value::as_str) else {
            return Err(StatusCode::BAD_REQUEST);
        };
        let matches = match expected {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => false,
        };
        if !matches {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

async fn record_rejected_action(
    state: &ApiState,
    tenant_id: Uuid,
    action_type_id: Uuid,
    contract_snapshot: Option<serde_json::Value>,
    target_object_ids: &[Uuid],
    parameters: &serde_json::Value,
    reason: &str,
    headers: &HeaderMap,
    triggering_event_ref: Option<&serde_json::Value>,
) {
    let invocation = ActionInvocation {
        id: Uuid::new_v4(),
        tenant_id,
        action_type_id,
        target_object_ids: serde_json::json!(target_object_ids),
        parameters: parameters.clone(),
        outcome: format!("Rejected: {reason}"),
        triggering_event_ref: {
            let mut context =
                triggering_event_ref.cloned().unwrap_or_else(|| serde_json::json!({}));
            if let Some(object) = context.as_object_mut() {
                object.entry("source").or_insert_with(|| serde_json::json!("console"));
                object.insert(
                    "actor".to_string(),
                    serde_json::json!(headers
                        .get("x-username")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("unknown")),
                );
                object.insert("reason".to_string(), serde_json::json!(reason));
            }
            context
        },
        contract_snapshot,
        executed_at: chrono::Utc::now(),
    };
    let _ = state.repository.insert_action_invocation(invocation).await;
}

async fn invoke_action(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(input): Json<InvokeActionInput>,
) -> Result<Json<ActionInvocation>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    if input.target_object_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let action_type = state
        .repository
        .list_action_types(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|action| action.id == input.action_type_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Err(status) =
        validate_action_parameters(&action_type.parameter_schema, &input.parameters)
    {
        record_rejected_action(
            &state,
            tenant_id,
            action_type.id,
            Some(serde_json::to_value(&action_type).unwrap_or_default()),
            &input.target_object_ids,
            &input.parameters,
            "parameter validation failed",
            &headers,
            input.triggering_event_ref.as_ref(),
        )
        .await;
        return Err(status);
    }
    let mut targets = Vec::with_capacity(input.target_object_ids.len());
    for id in &input.target_object_ids {
        match state
            .repository
            .get_object(tenant_id, *id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            Some(target) => targets.push(target),
            None => {
                record_rejected_action(
                    &state,
                    tenant_id,
                    action_type.id,
                    Some(serde_json::to_value(&action_type).unwrap_or_default()),
                    &input.target_object_ids,
                    &input.parameters,
                    "target object not found",
                    &headers,
                    input.triggering_event_ref.as_ref(),
                )
                .await;
                return Err(StatusCode::NOT_FOUND);
            }
        }
    }
    if let Some(expected_type_id) = action_type.target_object_type_id {
        for target in &targets {
            if target.object_type_id != expected_type_id {
                record_rejected_action(
                    &state,
                    tenant_id,
                    action_type.id,
                    Some(serde_json::to_value(&action_type).unwrap_or_default()),
                    &input.target_object_ids,
                    &input.parameters,
                    "target object type mismatch",
                    &headers,
                    input.triggering_event_ref.as_ref(),
                )
                .await;
                return Err(StatusCode::CONFLICT);
            }
        }
    }
    if let Some(preconditions) = action_type.preconditions.as_object() {
        for target in &targets {
            let Some(properties) = target.properties.as_object() else {
                record_rejected_action(
                    &state,
                    tenant_id,
                    action_type.id,
                    Some(serde_json::to_value(&action_type).unwrap_or_default()),
                    &input.target_object_ids,
                    &input.parameters,
                    "target properties are not an object",
                    &headers,
                    input.triggering_event_ref.as_ref(),
                )
                .await;
                return Err(StatusCode::CONFLICT);
            };
            if preconditions.iter().any(|(key, expected)| properties.get(key) != Some(expected)) {
                record_rejected_action(
                    &state,
                    tenant_id,
                    action_type.id,
                    Some(serde_json::to_value(&action_type).unwrap_or_default()),
                    &input.target_object_ids,
                    &input.parameters,
                    "preconditions not satisfied",
                    &headers,
                    input.triggering_event_ref.as_ref(),
                )
                .await;
                return Err(StatusCode::CONFLICT);
            }
        }
    } else if !action_type.preconditions.is_null() {
        record_rejected_action(
            &state,
            tenant_id,
            action_type.id,
            Some(serde_json::to_value(&action_type).unwrap_or_default()),
            &input.target_object_ids,
            &input.parameters,
            "invalid precondition contract",
            &headers,
            input.triggering_event_ref.as_ref(),
        )
        .await;
        return Err(StatusCode::CONFLICT);
    }
    let effects = action_type.effect_definition.as_object().ok_or(StatusCode::CONFLICT)?;
    let resolved_effects = effects
        .iter()
        .map(|(key, value)| Ok((key.clone(), resolve_effect_value(value, &input.parameters)?)))
        .collect::<Result<serde_json::Map<_, _>, StatusCode>>()?;
    for mut target in targets {
        let Some(properties) = target.properties.as_object_mut() else {
            return Err(StatusCode::CONFLICT);
        };
        for (key, value) in &resolved_effects {
            properties.insert(key.clone(), value.clone());
        }
        target.updated_at = chrono::Utc::now();
        state
            .repository
            .update_object(target)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let invocation = ActionInvocation {
        id: Uuid::new_v4(),
        tenant_id,
        action_type_id: action_type.id,
        target_object_ids: serde_json::to_value(&input.target_object_ids)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        parameters: input.parameters,
        outcome: "Completed".to_string(),
        triggering_event_ref: {
            let mut context = input.triggering_event_ref.unwrap_or_else(|| serde_json::json!({}));
            if let Some(object) = context.as_object_mut() {
                object.entry("source").or_insert_with(|| serde_json::json!("console"));
                object.insert(
                    "actor".to_string(),
                    serde_json::json!(headers
                        .get("x-username")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("unknown")),
                );
            }
            context
        },
        contract_snapshot: Some(
            serde_json::to_value(&action_type).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
        executed_at: chrono::Utc::now(),
    };
    state
        .repository
        .insert_action_invocation(invocation.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(invocation))
}

async fn create_action_type(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(input): Json<ActionTypeInput>,
) -> Result<Json<ActionType>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let now = chrono::Utc::now();
    let value = ActionType {
        id: Uuid::new_v4(),
        tenant_id,
        name: input.name,
        target_object_type_id: input.target_object_type_id,
        parameter_schema: input.parameter_schema,
        preconditions: input.preconditions,
        effect_definition: input.effect_definition,
        created_at: now,
        updated_at: now,
    };
    state.repository.create_action_type(value.clone()).await.map_err(|_| StatusCode::CONFLICT)?;
    state
        .repository
        .record_action_type_history(action_history(
            &value,
            "created",
            headers.get("x-username").and_then(|h| h.to_str().ok()).unwrap_or("unknown"),
            None,
            Some(serde_json::to_value(&value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?),
        ))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(value))
}

async fn delete_action_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let before = state
        .repository
        .list_action_types(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|action| action.id == id);
    state.repository.delete_action_type(tenant_id, id).await.map_err(|_| StatusCode::CONFLICT)?;
    if let Some(action) = before {
        state
            .repository
            .record_action_type_history(action_history(
                &action,
                "deleted",
                headers.get("x-username").and_then(|h| h.to_str().ok()).unwrap_or("unknown"),
                Some(serde_json::to_value(&action).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?),
                None,
            ))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_action_type(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ActionTypeInput>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let before = state
        .repository
        .list_action_types(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|action| action.id == id);
    let now = chrono::Utc::now();
    let value = ActionType {
        id,
        tenant_id,
        name: input.name,
        target_object_type_id: input.target_object_type_id,
        parameter_schema: input.parameter_schema,
        preconditions: input.preconditions,
        effect_definition: input.effect_definition,
        created_at: now,
        updated_at: now,
    };
    state
        .repository
        .update_action_type(value.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .repository
        .record_action_type_history(action_history(
            &value,
            "updated",
            headers.get("x-username").and_then(|h| h.to_str().ok()).unwrap_or("unknown"),
            before.map(|old| serde_json::to_value(old).unwrap_or_default()),
            Some(serde_json::to_value(&value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?),
        ))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
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

    let types = state
        .repository
        .get_object_types(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

    let obj_type = state
        .repository
        .get_object_type(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
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

    let types = state
        .repository
        .list_link_types(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

    let objects = state
        .repository
        .list_objects(tenant_id, query.object_type_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

    let obj = state
        .repository
        .get_object(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
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

    let (links, targets) = state
        .repository
        .traverse_links(tenant_id, id, link_type_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

    let actions = state
        .repository
        .list_action_invocations(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(actions))
}

async fn list_action_reviews(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<ActionReview>>, StatusCode> {
    let tenant_id = tenant(&headers)?;
    let reviews = state
        .repository
        .list_action_reviews(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(reviews))
}

#[derive(Deserialize)]
struct ActionReviewInput {
    invocation_id: Uuid,
    status: String,
    assignee: Option<String>,
    note: String,
    due_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn upsert_action_review(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
    Json(input): Json<ActionReviewInput>,
) -> Result<Json<ActionReview>, StatusCode> {
    let tenant_id = write_check(&headers)?;
    let status = input.status.trim().to_ascii_lowercase();
    if !matches!(status.as_str(), "open" | "in_progress" | "approved" | "declined" | "handed_off") {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if matches!(status.as_str(), "approved" | "declined") && !can_approve_action_review(&headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !state
        .repository
        .list_action_invocations(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .any(|invocation| invocation.id == input.invocation_id)
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let existing = state
        .repository
        .list_action_reviews(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|review| review.invocation_id == input.invocation_id);
    let now = chrono::Utc::now();
    let due_at = if matches!(status.as_str(), "approved" | "declined") {
        None
    } else {
        input
            .due_at
            .or_else(|| existing.as_ref().and_then(|review| review.due_at))
            .or_else(|| Some(now + chrono::Duration::minutes(30)))
    };
    let review = ActionReview {
        id: existing.as_ref().map(|review| review.id).unwrap_or_else(Uuid::new_v4),
        tenant_id,
        invocation_id: input.invocation_id,
        status,
        assignee: input
            .assignee
            .and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_string())),
        note: input.note.trim().to_string(),
        reviewed_by: actor(&headers),
        due_at,
        created_at: existing.as_ref().map(|review| review.created_at).unwrap_or(now),
        updated_at: now,
    };
    state
        .repository
        .upsert_action_review(review.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(review))
}
