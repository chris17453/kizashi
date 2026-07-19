use super::*;
use crate::execution_repository::execution_repository_test::{
    FailingExecutionRepository, InMemoryExecutionRepository,
};
use axum::body::Body;
use axum::http::Request;
use common::ActionExecution;
use tower::ServiceExt;

fn state(execution_repository: Arc<dyn ExecutionRepository>) -> ExecutionState {
    ExecutionState { execution_repository }
}

#[tokio::test]
async fn list_executions_returns_executions_for_the_event_scoped_to_tenant() {
    let repo = Arc::new(InMemoryExecutionRepository::default());
    let tenant_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let execution = ActionExecution::new(
        tenant_id,
        Uuid::new_v4(),
        event_id,
        common::ActionType::Webhook,
        serde_json::json!({}),
    );
    repo.insert(&execution).await.unwrap();

    let app = build_router(state(repo));
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/action-executions?event_id={event_id}"))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let executions: Vec<ActionExecution> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].id, execution.id);
}

#[tokio::test]
async fn list_executions_requires_tenant_header() {
    let repo = Arc::new(InMemoryExecutionRepository::default());
    let app = build_router(state(repo));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/action-executions?event_id={}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_executions_returns_500_on_backend_failure() {
    let repo = Arc::new(FailingExecutionRepository);
    let app = build_router(state(repo));

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/action-executions?event_id={}", Uuid::new_v4()))
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
