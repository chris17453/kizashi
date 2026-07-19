use super::*;
use crate::event_publisher::event_publisher_test::InMemoryEventPublisher;
use crate::event_store::event_store_test::{FailingEventStore, InMemoryEventStore};
use crate::signal_repository::signal_repository_test::{
    FailingSignalRepository, InMemorySignalRepository,
};
use crate::trigger_repository::trigger_repository_test::InMemoryTriggerRepository;
use common::{RawRecord, SourceType, ThresholdDirection, TriggerCondition, TriggerDefinition};
use serde_json::json;

fn deps(
    trigger_repo: InMemoryTriggerRepository,
) -> (TriggerDeps, Arc<InMemoryEventStore>, Arc<InMemoryEventPublisher>) {
    let event_store = Arc::new(InMemoryEventStore::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let deps = TriggerDeps {
        trigger_repository: Arc::new(trigger_repo),
        signal_repository: Arc::new(InMemorySignalRepository::default()),
        event_store: event_store.clone(),
        publisher: publisher.clone(),
    };
    (deps, event_store, publisher)
}

fn analyzed_record(
    tenant_id: Uuid,
    entity_ref: &str,
    analysis: serde_json::Value,
) -> AnalyzedRecord {
    let mut raw = RawRecord::new("zendesk", SourceType::Ticket, tenant_id, json!({}));
    raw.normalized_payload = Some(json!({"entity_ref": entity_ref}));
    AnalyzedRecord::new(raw, analysis)
}

fn threshold_trigger(tenant_id: Uuid) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "very negative sentiment".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::ThresholdOverWindow {
            field: "sentiment".to_string(),
            threshold: -0.5,
            direction: ThresholdDirection::Below,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn no_matching_trigger_records_the_signal_but_creates_no_event() {
    let tenant_id = Uuid::new_v4();
    let (deps, event_store, _publisher) = deps(InMemoryTriggerRepository::default());
    let record = analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.8}));

    let created = process_analyzed_record(&deps, &record).await.unwrap();

    assert_eq!(created, 0);
    assert!(event_store.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_firing_threshold_trigger_writes_and_publishes_an_event() {
    let tenant_id = Uuid::new_v4();
    let (deps, event_store, publisher) =
        deps(InMemoryTriggerRepository::with_trigger(threshold_trigger(tenant_id)));
    let record = analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.8}));

    let created = process_analyzed_record(&deps, &record).await.unwrap();

    assert_eq!(created, 1);
    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "sentiment");
    assert_eq!(events[0].group_key, "cust-1");
    assert_eq!(
        events[0].record_ids,
        vec![record.record.id],
        "the fired event must carry the id of the record whose signal satisfied the condition"
    );
    assert_eq!(publisher.published.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_non_firing_threshold_trigger_creates_no_event() {
    let tenant_id = Uuid::new_v4();
    let (deps, event_store, _publisher) =
        deps(InMemoryTriggerRepository::with_trigger(threshold_trigger(tenant_id)));
    let record = analyzed_record(tenant_id, "cust-1", json!({"sentiment": 0.1}));

    let created = process_analyzed_record(&deps, &record).await.unwrap();

    assert_eq!(created, 0);
    assert!(event_store.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_count_over_window_trigger_fires_only_once_the_threshold_count_is_reached() {
    let tenant_id = Uuid::new_v4();
    let trigger = TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "three complaints".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 2 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    };
    let (deps, event_store, _publisher) = deps(InMemoryTriggerRepository::with_trigger(trigger));

    let record1 = analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.2}));
    let first = process_analyzed_record(&deps, &record1).await.unwrap();
    assert_eq!(first, 0, "only one signal recorded so far");

    let record2 = analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.3}));
    let second = process_analyzed_record(&deps, &record2).await.unwrap();
    assert_eq!(second, 1, "second signal reaches the count-over-window threshold");
    let events = event_store.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let mut record_ids = events[0].record_ids.clone();
    record_ids.sort();
    let mut expected = vec![record1.record.id, record2.record.id];
    expected.sort();
    assert_eq!(
        record_ids, expected,
        "the fired event must carry both records whose signals reached the count threshold"
    );
}

#[tokio::test]
async fn different_group_keys_have_independent_windows() {
    let tenant_id = Uuid::new_v4();
    let trigger = TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id,
        name: "two complaints".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 2 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    };
    let (deps, event_store, _publisher) = deps(InMemoryTriggerRepository::with_trigger(trigger));

    process_analyzed_record(
        &deps,
        &analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.2})),
    )
    .await
    .unwrap();
    let created = process_analyzed_record(
        &deps,
        &analyzed_record(tenant_id, "cust-2", json!({"sentiment": -0.2})),
    )
    .await
    .unwrap();

    assert_eq!(created, 0, "cust-2's own window only has one signal, independent of cust-1's");
    assert!(event_store.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn candidates_with_non_numeric_analysis_produce_no_signals_or_events() {
    let tenant_id = Uuid::new_v4();
    let (deps, event_store, _publisher) =
        deps(InMemoryTriggerRepository::with_trigger(threshold_trigger(tenant_id)));
    let record = analyzed_record(tenant_id, "cust-1", json!({"summary": "no numbers here"}));

    let created = process_analyzed_record(&deps, &record).await.unwrap();

    assert_eq!(created, 0);
    assert!(event_store.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn propagates_signal_record_failure() {
    let tenant_id = Uuid::new_v4();
    let deps = TriggerDeps {
        trigger_repository: Arc::new(InMemoryTriggerRepository::default()),
        signal_repository: Arc::new(FailingSignalRepository),
        event_store: Arc::new(InMemoryEventStore::default()),
        publisher: Arc::new(InMemoryEventPublisher::default()),
    };
    let record = analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.8}));

    let err = process_analyzed_record(&deps, &record).await.unwrap_err();
    assert!(matches!(err, ProcessError::SignalRecord(_)));
}

#[tokio::test]
async fn propagates_event_write_failure_when_a_trigger_fires() {
    let tenant_id = Uuid::new_v4();
    let deps = TriggerDeps {
        trigger_repository: Arc::new(InMemoryTriggerRepository::with_trigger(threshold_trigger(
            tenant_id,
        ))),
        signal_repository: Arc::new(InMemorySignalRepository::default()),
        event_store: Arc::new(FailingEventStore),
        publisher: Arc::new(InMemoryEventPublisher::default()),
    };
    let record = analyzed_record(tenant_id, "cust-1", json!({"sentiment": -0.8}));

    let err = process_analyzed_record(&deps, &record).await.unwrap_err();
    assert!(matches!(err, ProcessError::EventWrite(_)));
}
