use super::*;
use common::{RawRecord, SourceType};
use serde_json::json;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryEventPublisher {
    pub published: Mutex<Vec<AnalyzedRecord>>,
}

#[async_trait]
impl EventPublisher for InMemoryEventPublisher {
    async fn publish_record_analyzed(&self, record: &AnalyzedRecord) -> Result<(), PublishError> {
        self.published.lock().unwrap().push(record.clone());
        Ok(())
    }
}

pub struct FailingEventPublisher;

#[async_trait]
impl EventPublisher for FailingEventPublisher {
    async fn publish_record_analyzed(&self, _record: &AnalyzedRecord) -> Result<(), PublishError> {
        Err(PublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_analyzed_record() -> AnalyzedRecord {
    let record = RawRecord::new("zendesk", SourceType::Ticket, Uuid::new_v4(), json!({}));
    AnalyzedRecord::new(record, json!({"sentiment": -0.5}))
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemoryEventPublisher::default();
    let record = sample_analyzed_record();

    publisher.publish_record_analyzed(&record).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], record);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingEventPublisher;
    let err = publisher.publish_record_analyzed(&sample_analyzed_record()).await.unwrap_err();
    assert!(matches!(err, PublishError::Bus(_)));
}
