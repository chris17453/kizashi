use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryEventPublisher {
    pub published: Mutex<Vec<RawRecord>>,
}

#[async_trait]
impl EventPublisher for InMemoryEventPublisher {
    async fn publish_record_normalized(&self, record: &RawRecord) -> Result<(), PublishError> {
        self.published.lock().unwrap().push(record.clone());
        Ok(())
    }
}

pub struct FailingEventPublisher;

#[async_trait]
impl EventPublisher for FailingEventPublisher {
    async fn publish_record_normalized(&self, _record: &RawRecord) -> Result<(), PublishError> {
        Err(PublishError::Bus("simulated bus failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemoryEventPublisher::default();
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );

    publisher.publish_record_normalized(&record).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], record);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingEventPublisher;
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );

    let err = publisher.publish_record_normalized(&record).await.unwrap_err();
    assert!(matches!(err, PublishError::Bus(_)));
}
