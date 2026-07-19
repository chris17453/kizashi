use super::*;
use common::Sensor;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemorySensorPublisher {
    pub published: Mutex<Vec<SensorChangeEvent>>,
}

#[async_trait]
impl SensorPublisher for InMemorySensorPublisher {
    async fn publish_sensor_changed(
        &self,
        event: &SensorChangeEvent,
    ) -> Result<(), SensorPublishError> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub struct FailingSensorPublisher;

#[async_trait]
impl SensorPublisher for FailingSensorPublisher {
    async fn publish_sensor_changed(
        &self,
        _event: &SensorChangeEvent,
    ) -> Result<(), SensorPublishError> {
        Err(SensorPublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_event() -> SensorChangeEvent {
    SensorChangeEvent::Upserted(Sensor::new(
        Uuid::new_v4(),
        "zendesk",
        "support-poller",
        serde_json::json!({}),
    ))
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemorySensorPublisher::default();
    let event = sample_event();

    publisher.publish_sensor_changed(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingSensorPublisher;
    let err = publisher.publish_sensor_changed(&sample_event()).await.unwrap_err();
    assert!(matches!(err, SensorPublishError::Bus(_)));
}
