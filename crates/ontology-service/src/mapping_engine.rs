#[path = "mapping_engine_test.rs"]
#[cfg(test)]
mod mapping_engine_test;

use common::ontology::Object;
use common::RawRecord;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use std::sync::Arc;
use crate::repository::{OntologyRepository, RepositoryError};

pub struct OntologyMappingEngine {
    pub repository: Arc<dyn OntologyRepository>,
}

impl OntologyMappingEngine {
    pub fn new(repository: Arc<dyn OntologyRepository>) -> Self {
        Self { repository }
    }

    pub async fn process_record(&self, record: RawRecord) -> Result<(), RepositoryError> {
        if record.normalized_payload.is_none() {
            return Ok(());
        }

        let normalized = record.normalized_payload.as_ref().unwrap();

        let object_types = self.repository.get_object_types(record.tenant_id).await?;

        let source_type_str = match record.source_type {
            common::SourceType::Message => "message",
            common::SourceType::Ticket => "ticket",
            common::SourceType::Log => "log",
            common::SourceType::SqlRow => "sql_row",
            common::SourceType::FabricRecord => "fabric_record",
            common::SourceType::Generic => "generic",
        };

        for ot in object_types {
            if let Some(rules) = ot.mapping_rules.as_array() {
                for rule in rules {
                    if rule.get("source_type").and_then(|v| v.as_str()) == Some(source_type_str) {
                        let mut properties = serde_json::Map::new();
                        let mut identity_val = None;

                        if let Some(mappings) = rule.get("fields").and_then(|v| v.as_object()) {
                            for (target_prop, source_path) in mappings {
                                if let Some(path) = source_path.as_str() {
                                    if let Some(val) = normalized.get(path) {
                                        properties.insert(target_prop.clone(), val.clone());
                                        if rule.get("identity_field").and_then(|v| v.as_str())
                                            == Some(target_prop.as_str())
                                        {
                                            identity_val = Some(val.clone());
                                        }
                                    }
                                }
                            }
                        }

                        let obj_id = if let Some(id_val) = identity_val {
                            let hash_input = format!("{}:{}:{}", record.tenant_id, ot.id, id_val);
                            let hash = Sha256::digest(hash_input.as_bytes());
                            Uuid::from_bytes(hash[..16].try_into().unwrap())
                        } else {
                            let hash_input =
                                format!("{}:{}:{}", record.tenant_id, ot.id, record.id);
                            let hash = Sha256::digest(hash_input.as_bytes());
                            Uuid::from_bytes(hash[..16].try_into().unwrap())
                        };

                        let object = Object {
                            id: obj_id,
                            tenant_id: record.tenant_id,
                            object_type_id: ot.id,
                            properties: serde_json::Value::Object(properties),
                            source_lineage: serde_json::json!([record.id]),
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                        };

                        self.repository.upsert_object(object).await?;
                    }
                }
            }
        }

        Ok(())
    }
}
