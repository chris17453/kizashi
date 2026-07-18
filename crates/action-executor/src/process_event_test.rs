use super::*;
use crate::action_dispatcher::action_dispatcher_test::{
    FailingActionDispatcher, InMemoryActionDispatcher,
};
use crate::execution_repository::execution_repository_test::{
    FailingExecutionRepository, InMemoryExecutionRepository,
};
use crate::trigger_client::trigger_client_test::{FailingTriggerClient, InMemoryTriggerClient};
use common::{ActionRef, ActionType, EventStatus, TriggerCondition, TriggerDefinition};
use serde_json::json;

fn event_with_trigger(trigger_id: Uuid) -> Event {
    Event {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        event_type: "sentiment".to_string(),
        source_connector_ids: vec![],
        entity_ref: "cust-1".to_string(),
        group_key: "cust-1".to_string(),
        payload: json!({"triggered_by": trigger_id, "value": -0.8}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: EventStatus::New,
    }
}

fn trigger_with_actions(actions: Vec<ActionRef>) -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "test".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 1 },
        window_seconds: 3600,
        actions,
        enabled: true,
    }
}

#[tokio::test]
async fn dispatches_every_action_and_writes_a_sent_execution_row_for_each() {
    let action = ActionRef {
        action_type: ActionType::Webhook,
        config: json!({"url": "http://example.test"}),
    };
    let trigger = trigger_with_actions(vec![action.clone(), action]);
    let event = event_with_trigger(trigger.id);

    let dispatcher = Arc::new(InMemoryActionDispatcher::default());
    let execution_repo = Arc::new(InMemoryExecutionRepository::default());
    let deps = ActionDeps {
        trigger_client: Arc::new(InMemoryTriggerClient::with_trigger(trigger.clone())),
        dispatcher: dispatcher.clone(),
        execution_repository: execution_repo.clone(),
    };

    let executed = process_event(&deps, &event).await.unwrap();

    assert_eq!(executed, 2);
    assert_eq!(dispatcher.dispatched.lock().unwrap().len(), 2);
    let executions = execution_repo.executions.lock().unwrap();
    assert_eq!(executions.len(), 2);
    assert!(executions.iter().all(|e| e.status == ActionExecutionStatus::Sent));
    assert!(executions.iter().all(|e| e.trigger_id == trigger.id && e.event_id == event.id));
}

#[tokio::test]
async fn a_dispatch_failure_is_recorded_as_a_failed_execution_not_swallowed() {
    let action = ActionRef {
        action_type: ActionType::Webhook,
        config: json!({"url": "http://example.test"}),
    };
    let trigger = trigger_with_actions(vec![action]);
    let event = event_with_trigger(trigger.id);

    let execution_repo = Arc::new(InMemoryExecutionRepository::default());
    let deps = ActionDeps {
        trigger_client: Arc::new(InMemoryTriggerClient::with_trigger(trigger)),
        dispatcher: Arc::new(FailingActionDispatcher),
        execution_repository: execution_repo.clone(),
    };

    let executed = process_event(&deps, &event).await.unwrap();

    assert_eq!(executed, 1);
    let executions = execution_repo.executions.lock().unwrap();
    assert_eq!(executions[0].status, ActionExecutionStatus::Failed);
}

#[tokio::test]
async fn event_with_no_triggered_by_field_is_rejected() {
    let mut event = event_with_trigger(Uuid::new_v4());
    event.payload = json!({"value": -0.8});

    let deps = ActionDeps {
        trigger_client: Arc::new(InMemoryTriggerClient::default()),
        dispatcher: Arc::new(InMemoryActionDispatcher::default()),
        execution_repository: Arc::new(InMemoryExecutionRepository::default()),
    };

    let err = process_event(&deps, &event).await.unwrap_err();
    assert!(matches!(err, ProcessError::MissingTriggerId));
}

#[tokio::test]
async fn event_pointing_at_an_unknown_trigger_is_rejected() {
    let trigger_id = Uuid::new_v4();
    let event = event_with_trigger(trigger_id);

    let deps = ActionDeps {
        trigger_client: Arc::new(InMemoryTriggerClient::default()),
        dispatcher: Arc::new(InMemoryActionDispatcher::default()),
        execution_repository: Arc::new(InMemoryExecutionRepository::default()),
    };

    let err = process_event(&deps, &event).await.unwrap_err();
    assert!(matches!(err, ProcessError::TriggerNotFound(id) if id == trigger_id));
}

#[tokio::test]
async fn trigger_with_no_actions_executes_nothing() {
    let trigger = trigger_with_actions(vec![]);
    let event = event_with_trigger(trigger.id);

    let deps = ActionDeps {
        trigger_client: Arc::new(InMemoryTriggerClient::with_trigger(trigger)),
        dispatcher: Arc::new(InMemoryActionDispatcher::default()),
        execution_repository: Arc::new(InMemoryExecutionRepository::default()),
    };

    let executed = process_event(&deps, &event).await.unwrap();
    assert_eq!(executed, 0);
}

#[tokio::test]
async fn propagates_execution_write_failure() {
    let action = ActionRef {
        action_type: ActionType::Webhook,
        config: json!({"url": "http://example.test"}),
    };
    let trigger = trigger_with_actions(vec![action]);
    let event = event_with_trigger(trigger.id);

    let deps = ActionDeps {
        trigger_client: Arc::new(InMemoryTriggerClient::with_trigger(trigger)),
        dispatcher: Arc::new(InMemoryActionDispatcher::default()),
        execution_repository: Arc::new(FailingExecutionRepository),
    };

    let err = process_event(&deps, &event).await.unwrap_err();
    assert!(matches!(err, ProcessError::ExecutionWrite(_)));
}

#[tokio::test]
async fn propagates_trigger_lookup_failure() {
    let event = event_with_trigger(Uuid::new_v4());
    let deps = ActionDeps {
        trigger_client: Arc::new(FailingTriggerClient),
        dispatcher: Arc::new(InMemoryActionDispatcher::default()),
        execution_repository: Arc::new(InMemoryExecutionRepository::default()),
    };

    let err = process_event(&deps, &event).await.unwrap_err();
    assert!(matches!(err, ProcessError::TriggerLookup(_)));
}
