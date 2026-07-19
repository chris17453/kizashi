use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryMappingPublisher {
    pub published: Mutex<Vec<NormalizationMapping>>,
}

#[async_trait]
impl MappingPublisher for InMemoryMappingPublisher {
    async fn publish_mapping_changed(
        &self,
        mapping: &NormalizationMapping,
    ) -> Result<(), MappingPublishError> {
        self.published.lock().unwrap().push(mapping.clone());
        Ok(())
    }
}

pub struct FailingMappingPublisher;

#[async_trait]
impl MappingPublisher for FailingMappingPublisher {
    async fn publish_mapping_changed(
        &self,
        _mapping: &NormalizationMapping,
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
async fn in_memory_publisher_records_published_mappings() {
    let publisher = InMemoryMappingPublisher::default();
    let mapping = sample_mapping();

    publisher.publish_mapping_changed(&mapping).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], mapping);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingMappingPublisher;
    let err = publisher.publish_mapping_changed(&sample_mapping()).await.unwrap_err();
    assert!(matches!(err, MappingPublishError::Bus(_)));
}
