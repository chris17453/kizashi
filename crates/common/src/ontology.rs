#[path = "ontology_test.rs"]
#[cfg(test)]
mod ontology_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectType {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub version: i32,
    pub property_schema: serde_json::Value,
    pub mapping_rules: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub object_type_id: Uuid,
    pub properties: serde_json::Value,
    pub source_lineage: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkType {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub source_object_type_id: Uuid,
    pub target_object_type_id: Uuid,
    pub cardinality: String,
    pub properties_schema: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub link_type_id: Uuid,
    pub source_object_id: Uuid,
    pub target_object_id: Uuid,
    pub properties: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionType {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub parameter_schema: serde_json::Value,
    pub preconditions: serde_json::Value,
    pub effect_definition: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionInvocation {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub action_type_id: Uuid,
    pub target_object_ids: serde_json::Value,
    pub parameters: serde_json::Value,
    pub outcome: String,
    pub triggering_event_ref: serde_json::Value,
    pub executed_at: DateTime<Utc>,
}
