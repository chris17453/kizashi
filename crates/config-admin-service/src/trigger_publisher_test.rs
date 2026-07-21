use super::*;
use common::trigger_definition::{ThresholdDirection, TriggerCondition};
use common::TriggerDefinition;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryTriggerPublisher {
    pub published: Mutex<Vec<TriggerChangeEvent>>,
}

#[async_trait]
impl TriggerPublisher for InMemoryTriggerPublisher {
    async fn publish_trigger_changed(
        &self,
        event: &TriggerChangeEvent,
    ) -> Result<(), TriggerPublishError> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub struct FailingTriggerPublisher;

#[async_trait]
impl TriggerPublisher for FailingTriggerPublisher {
    async fn publish_trigger_changed(
        &self,
        _event: &TriggerChangeEvent,
    ) -> Result<(), TriggerPublishError> {
        Err(TriggerPublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_trigger() -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "spike".to_string(),
        event_type_match: "priority_score".to_string(),
        condition: TriggerCondition::ThresholdOverWindow {
            field: "priority_score".to_string(),
            threshold: 5.0,
            direction: ThresholdDirection::Above,
        },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn in_memory_publisher_records_published_events() {
    let publisher = InMemoryTriggerPublisher::default();
    let event = TriggerChangeEvent::Upserted(sample_trigger());

    publisher.publish_trigger_changed(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn in_memory_publisher_records_deleted_events() {
    let publisher = InMemoryTriggerPublisher::default();
    let event = TriggerChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };

    publisher.publish_trigger_changed(&event).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published[0], event);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingTriggerPublisher;
    let event = TriggerChangeEvent::Upserted(sample_trigger());
    let err = publisher.publish_trigger_changed(&event).await.unwrap_err();
    assert!(matches!(err, TriggerPublishError::Bus(_)));
}
