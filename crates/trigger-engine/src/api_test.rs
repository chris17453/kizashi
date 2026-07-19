use super::*;
use crate::signal_repository::signal_repository_test::InMemorySignalRepository;
use crate::signal_repository::AnalyzedSignal;
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

fn state(
    trigger_repository: InMemoryTriggerRepository,
    signal_repository: InMemorySignalRepository,
) -> ApiState {
    ApiState {
        trigger_repository: Arc::new(trigger_repository),
        signal_repository: Arc::new(signal_repository),
    }
}

#[tokio::test]
async fn returns_200_and_the_trigger_when_found() {
    let trigger = sample_trigger();
    let state = state(
        InMemoryTriggerRepository::with_trigger(trigger.clone()),
        InMemorySignalRepository::default(),
    );

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
    let state = state(InMemoryTriggerRepository::default(), InMemorySignalRepository::default());

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

async fn send_test_request(
    api_state: ApiState,
    id: Uuid,
    tenant_id: Option<Uuid>,
    group_key: &str,
) -> axum::http::Response<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri(format!("/v1/triggers/{id}/test"))
        .header("content-type", "application/json");
    if let Some(tenant_id) = tenant_id {
        req = req.header("x-tenant-id", tenant_id.to_string());
    }
    build_router(api_state)
        .oneshot(
            req.body(Body::from(serde_json::json!({"group_key": group_key}).to_string())).unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn test_trigger_reports_would_fire_true_when_the_condition_is_already_satisfied() {
    let trigger = sample_trigger();
    let signal_repo = InMemorySignalRepository::default();
    for _ in 0..3 {
        signal_repo
            .record_signal(&AnalyzedSignal {
                id: Uuid::new_v4(),
                tenant_id: trigger.tenant_id,
                record_id: Uuid::new_v4(),
                event_type: trigger.event_type_match.clone(),
                group_key: "cust-1".to_string(),
                entity_ref: "cust-1".to_string(),
                numeric_value: None,
                source_connector_id: "zendesk".to_string(),
                occurred_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
    }
    let api_state = state(InMemoryTriggerRepository::with_trigger(trigger.clone()), signal_repo);

    let response =
        send_test_request(api_state, trigger.id, Some(trigger.tenant_id), "cust-1").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["would_fire"], true);
    assert_eq!(body["contributing_record_count"], 3);
}

#[tokio::test]
async fn test_trigger_reports_would_fire_false_when_not_enough_signals_exist() {
    let trigger = sample_trigger();
    let api_state = state(
        InMemoryTriggerRepository::with_trigger(trigger.clone()),
        InMemorySignalRepository::default(),
    );

    let response =
        send_test_request(api_state, trigger.id, Some(trigger.tenant_id), "cust-1").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["would_fire"], false);
}

#[tokio::test]
async fn test_trigger_returns_404_for_a_tenant_mismatch() {
    let trigger = sample_trigger();
    let api_state = state(
        InMemoryTriggerRepository::with_trigger(trigger.clone()),
        InMemorySignalRepository::default(),
    );

    let response = send_test_request(api_state, trigger.id, Some(Uuid::new_v4()), "cust-1").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_trigger_requires_a_tenant_header() {
    let trigger = sample_trigger();
    let api_state = state(
        InMemoryTriggerRepository::with_trigger(trigger.clone()),
        InMemorySignalRepository::default(),
    );

    let response = send_test_request(api_state, trigger.id, None, "cust-1").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
