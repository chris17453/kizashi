use super::*;
use crate::InMemoryOntologyRepository;
use common::ontology::ObjectType;
use chrono::Utc;
use serde_json::json;

#[tokio::test]
async fn test_process_record_creates_object() {
    let repo = Arc::new(InMemoryOntologyRepository::new());
    let engine = OntologyMappingEngine::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let ot_id = Uuid::new_v4();

    repo.create_object_type(ObjectType {
        id: ot_id,
        tenant_id,
        name: "Customer".to_string(),
        version: 1,
        property_schema: json!({"name": "string"}),
        mapping_rules: json!([
            {
                "source_type": "ticket",
                "identity_field": "name",
                "fields": {
                    "name": "requester_name"
                }
            }
        ]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .await
    .unwrap();

    let record = RawRecord {
        id: Uuid::new_v4(),
        tenant_id,
        source_type: common::SourceType::Ticket,
        connector_id: Uuid::new_v4().to_string(),
        external_id: Some("src1".to_string()),
        raw_payload: json!({}),
        normalized_payload: Some(json!({
            "requester_name": "Alice"
        })),
        ingested_at: Utc::now(),
        occurred_at: Some(Utc::now()),
    };

    engine.process_record(record.clone()).await.unwrap();

    let objects = repo.get_objects(tenant_id).await.unwrap();
    assert_eq!(objects.len(), 1);
    let obj = &objects[0];
    assert_eq!(obj.object_type_id, ot_id);
    assert_eq!(obj.properties, json!({"name": "Alice"}));
    
    // Test idempotency/upsert
    let record2 = RawRecord {
        id: Uuid::new_v4(), // Different record ID
        tenant_id,
        source_type: common::SourceType::Ticket,
        connector_id: Uuid::new_v4().to_string(),
        external_id: Some("src1".to_string()),
        raw_payload: json!({}),
        normalized_payload: Some(json!({
            "requester_name": "Alice" // Same identity value
        })),
        ingested_at: Utc::now(),
        occurred_at: Some(Utc::now()),
    };
    
    engine.process_record(record2).await.unwrap();
    let objects_after = repo.get_objects(tenant_id).await.unwrap();
    assert_eq!(objects_after.len(), 1); // Should still be 1 object
    assert_eq!(objects_after[0].id, obj.id);
}
