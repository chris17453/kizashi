use super::*;
use crate::trigger_repository::trigger_repository_test::InMemoryTriggerRepository;
use axum::body::Body;
use axum::http::Request;
use common::TriggerDefinition;
use tower::ServiceExt;

fn sample_trigger() -> TriggerDefinition {
    TriggerDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "test".to_string(),
        event_type_match: "sentiment".to_string(),
        condition: common::TriggerCondition::CountOverWindow { count: 3 },
        window_seconds: 3600,
        actions: vec![],
        enabled: true,
    }
}

#[tokio::test]
async fn returns_200_and_the_trigger_when_found() {
    let trigger = sample_trigger();
    let state = ApiState {
        trigger_repository: Arc::new(InMemoryTriggerRepository::with_trigger(trigger.clone())),
    };

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/triggers/{}", trigger.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let found: TriggerDefinition = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(found, trigger);
}

#[tokio::test]
async fn returns_404_when_not_found() {
    let state = ApiState { trigger_repository: Arc::new(InMemoryTriggerRepository::default()) };

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/triggers/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
