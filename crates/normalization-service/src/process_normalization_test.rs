use super::*;
use crate::event_publisher::event_publisher_test::{FailingEventPublisher, InMemoryEventPublisher};
use crate::mapping_repository::mapping_repository_test::InMemoryMappingRepository;
use crate::record_client::record_client_test::{FailingRecordClient, InMemoryRecordClient};
use common::{NormalizationMapping, SourceType};
use std::collections::BTreeMap;
use uuid::Uuid;

fn mapping_for(tenant_id: Uuid) -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping::new(tenant_id, "ticket", field_map)
}

fn sample_record(tenant_id: Uuid) -> RawRecord {
    RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        serde_json::json!({"description": "printer on fire"}),
    )
}

#[test]
fn source_type_key_matches_normalization_mapping_convention() {
    assert_eq!(source_type_key(SourceType::Ticket), "ticket");
    assert_eq!(source_type_key(SourceType::SqlRow), "sql_row");
}

#[tokio::test]
async fn normalizes_writes_back_and_publishes_when_a_mapping_exists() {
    let tenant_id = Uuid::new_v4();
    let record_client = Arc::new(InMemoryRecordClient::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let deps = NormalizationDeps {
        mapping_repository: Arc::new(InMemoryMappingRepository::with_mapping(mapping_for(
            tenant_id,
        ))),
        record_client: record_client.clone(),
        publisher: publisher.clone(),
    };
    let record = sample_record(tenant_id);

    let outcome = process_normalization(&deps, &record).await.unwrap();

    assert_eq!(outcome, ProcessOutcome::Normalized);
    let updates = record_client.updates.lock().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].0, record.id);
    assert_eq!(updates[0].1, serde_json::json!({"text": "printer on fire"}));

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].normalized_payload,
        Some(serde_json::json!({"text": "printer on fire"}))
    );
}

#[tokio::test]
async fn skips_without_error_when_no_mapping_is_configured() {
    let tenant_id = Uuid::new_v4();
    let record_client = Arc::new(InMemoryRecordClient::default());
    let deps = NormalizationDeps {
        mapping_repository: Arc::new(InMemoryMappingRepository::default()),
        record_client: record_client.clone(),
        publisher: Arc::new(InMemoryEventPublisher::default()),
    };

    let outcome = process_normalization(&deps, &sample_record(tenant_id)).await.unwrap();

    assert_eq!(outcome, ProcessOutcome::NoMappingConfigured);
    assert!(record_client.updates.lock().unwrap().is_empty());
}

#[tokio::test]
async fn propagates_error_when_record_client_fails() {
    let tenant_id = Uuid::new_v4();
    let deps = NormalizationDeps {
        mapping_repository: Arc::new(InMemoryMappingRepository::with_mapping(mapping_for(
            tenant_id,
        ))),
        record_client: Arc::new(FailingRecordClient),
        publisher: Arc::new(InMemoryEventPublisher::default()),
    };

    let err = process_normalization(&deps, &sample_record(tenant_id)).await.unwrap_err();
    assert!(matches!(err, ProcessError::RecordUpdate(_)));
}

#[tokio::test]
async fn publish_failure_does_not_fail_the_overall_outcome() {
    let tenant_id = Uuid::new_v4();
    let record_client = Arc::new(InMemoryRecordClient::default());
    let deps = NormalizationDeps {
        mapping_repository: Arc::new(InMemoryMappingRepository::with_mapping(mapping_for(
            tenant_id,
        ))),
        record_client: record_client.clone(),
        publisher: Arc::new(FailingEventPublisher),
    };

    let outcome = process_normalization(&deps, &sample_record(tenant_id)).await.unwrap();

    assert_eq!(outcome, ProcessOutcome::Normalized);
    assert_eq!(record_client.updates.lock().unwrap().len(), 1, "write-back must still happen");
}
