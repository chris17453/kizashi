use super::*;
use common::NormalizationMapping;
use std::collections::BTreeMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryMappingPublisher {
    pub published: Mutex<Vec<MappingChangeEvent>>,
}

#[async_trait]
impl MappingPublisher for InMemoryMappingPublisher {
    async fn publish_mapping_changed(
        &self,
        event: &MappingChangeEvent,
    ) -> Result<(), MappingPublishError> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub struct FailingMappingPublisher;

#[async_trait]
impl MappingPublisher for FailingMappingPublisher {
    async fn publish_mapping_changed(
        &self,
        _event: &MappingChangeEvent,
    ) -> Result<(), MappingPublishError> {
        Err(MappingPublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_mapping() -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping::new(Uuid::new_v4(), "ticket", field_map)
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemoryMappingPublisher::default();
    let event = MappingChangeEvent::Upserted(sample_mapping());

    publisher.publish_mapping_changed(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn in_memory_publisher_records_deleted_events() {
    let publisher = InMemoryMappingPublisher::default();
    let event = MappingChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };

    publisher.publish_mapping_changed(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingMappingPublisher;
    let event = MappingChangeEvent::Upserted(sample_mapping());
    let err = publisher.publish_mapping_changed(&event).await.unwrap_err();
    assert!(matches!(err, MappingPublishError::Bus(_)));
}
