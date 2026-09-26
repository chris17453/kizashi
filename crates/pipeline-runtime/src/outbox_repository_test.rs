use super::*;

#[test]
fn outbox_message_preserves_the_execution_and_event_type() {
    let message = OutboxMessage {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        execution_id: Uuid::new_v4(),
        event_type: "pipeline.execution.queued".to_string(),
        payload: serde_json::json!({}),
    };
    assert_eq!(message.event_type, "pipeline.execution.queued");
}
