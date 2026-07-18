use super::*;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryEventPublisher {
    pub published: Mutex<Vec<Event>>,
}

#[async_trait]
impl EventPublisher for InMemoryEventPublisher {
    async fn publish_event_created(&self, event: &Event) -> Result<(), PublishError> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub struct FailingEventPublisher;

#[async_trait]
impl EventPublisher for FailingEventPublisher {
    async fn publish_event_created(&self, _event: &Event) -> Result<(), PublishError> {
        Err(PublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_event() -> Event {
    Event::new(
        Uuid::new_v4(),
        "sentiment",
        "cust-1",
        "cust-1",
        serde_json::json!({"sentiment": -0.8}),
        chrono::Utc::now(),
    )
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemoryEventPublisher::default();
    let event = sample_event();

    publisher.publish_event_created(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingEventPublisher;
    let err = publisher.publish_event_created(&sample_event()).await.unwrap_err();
    assert!(matches!(err, PublishError::Bus(_)));
}
