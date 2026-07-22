#[path = "mapping_engine_test.rs"]
#[cfg(test)]
mod mapping_engine_test;

use common::ontology::ObjectType;
use common::RawRecord;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct OntologyMappingEngine {
    pool: PgPool,
}

type ObjectTypeRow = (
    Uuid,
    Uuid,
    String,
    i32,
    serde_json::Value,
    serde_json::Value,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

impl OntologyMappingEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn process_record(&self, record: RawRecord) -> Result<(), sqlx::Error> {
        if record.normalized_payload.is_none() {
            return Ok(());
        }

        let normalized = record.normalized_payload.as_ref().unwrap();

        // 1. Fetch object types and their mapping rules
        let rows: Vec<ObjectTypeRow> = sqlx::query_as(
            "SELECT id, tenant_id, name, version, property_schema, mapping_rules, created_at, updated_at FROM object_types WHERE tenant_id = $1",
        )
        .bind(record.tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let object_types = rows
            .into_iter()
            .map(|row| ObjectType {
                id: row.0,
                tenant_id: row.1,
                name: row.2,
                version: row.3,
                property_schema: row.4,
                mapping_rules: row.5,
                created_at: row.6,
                updated_at: row.7,
            })
            .collect::<Vec<_>>();

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
                        if let Some(mappings) = rule.get("fields").and_then(|v| v.as_object()) {
                            for (target_prop, source_path) in mappings {
                                if let Some(path) = source_path.as_str() {
                                    if let Some(val) = normalized.get(path) {
                                        properties.insert(target_prop.clone(), val.clone());
                                    }
                                }
                            }
                        }

                        let obj_id = Uuid::new_v4();

                        sqlx::query(
                            "INSERT INTO objects (id, tenant_id, object_type_id, properties, source_lineage) 
                             VALUES ($1, $2, $3, $4, $5)"
                        )
                        .bind(obj_id)
                        .bind(record.tenant_id)
                        .bind(ot.id)
                        .bind(serde_json::Value::Object(properties))
                        .bind(serde_json::json!([record.id]))
                        .execute(&self.pool)
                        .await?;
                    }
                }
            }
        }

        Ok(())
    }
}
