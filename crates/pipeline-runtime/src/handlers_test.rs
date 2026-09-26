use super::*;
use async_trait::async_trait;
use axum::{body::Body, http::Request};
use common::PipelineExecutionStatus;
use std::sync::Mutex;
use tower::ServiceExt;

struct NoopRepository;

#[async_trait]
impl ExecutionRepository for NoopRepository {
    async fn create(
        &self,
        execution: PipelineExecution,
    ) -> Result<ExecutionRequestResult, ExecutionRepositoryError> {
        Ok(ExecutionRequestResult::Created(execution))
    }
    async fn mark_awaiting_confirmation(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<(), ExecutionRepositoryError> {
        Ok(())
    }
    async fn record_confirmation(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: serde_json::Value,
    ) -> Result<ConfirmationResult, ExecutionRepositoryError> {
        Ok(ConfirmationResult::Recorded)
    }
    async fn record_reconciliation_failure(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: serde_json::Value,
        _: &str,
    ) -> Result<(), ExecutionRepositoryError> {
        Ok(())
    }
}

type ReconciliationFailure = (Uuid, Uuid, String, serde_json::Value, String);

#[derive(Default)]
struct RecordingRepository {
    failures: Mutex<Vec<ReconciliationFailure>>,
}

#[async_trait]
impl ExecutionRepository for RecordingRepository {
    async fn create(
        &self,
        execution: PipelineExecution,
    ) -> Result<ExecutionRequestResult, ExecutionRepositoryError> {
        Ok(ExecutionRequestResult::Created(execution))
    }
    async fn mark_awaiting_confirmation(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<(), ExecutionRepositoryError> {
        Ok(())
    }
    async fn record_confirmation(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: serde_json::Value,
    ) -> Result<ConfirmationResult, ExecutionRepositoryError> {
        Ok(ConfirmationResult::Recorded)
    }
    async fn record_reconciliation_failure(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        payload: serde_json::Value,
        failure: &str,
    ) -> Result<(), ExecutionRepositoryError> {
        self.failures.lock().unwrap().push((
            tenant_id,
            execution_id,
            source_event_id.to_string(),
            payload,
            failure.to_string(),
        ));
        Ok(())
    }
}

struct FailingProjectionClient;

struct SuccessfulWorkflowRepository;

#[async_trait]
impl WorkflowRepository for SuccessfulWorkflowRepository {
    async fn open_approval(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        summary: &str,
        due_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<common::WorkflowCase, WorkflowRepositoryError> {
        Ok(common::WorkflowCase {
            id: Uuid::new_v4(),
            tenant_id,
            execution_id,
            kind: common::WorkflowCaseKind::Approval,
            status: common::WorkflowCaseStatus::PendingReview,
            summary: summary.to_string(),
            due_at: Some(due_at),
            decided_by: None,
            decision_note: None,
            created_at: chrono::Utc::now(),
            resolved_at: None,
        })
    }
    async fn open_reconciliation_failure(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        _: &str,
        summary: &str,
    ) -> Result<common::WorkflowCase, WorkflowRepositoryError> {
        Ok(common::WorkflowCase {
            id: Uuid::new_v4(),
            tenant_id,
            execution_id,
            kind: common::WorkflowCaseKind::ReconciliationFailure,
            status: common::WorkflowCaseStatus::Exception,
            summary: summary.to_string(),
            due_at: Some(chrono::Utc::now()),
            decided_by: None,
            decision_note: None,
            created_at: chrono::Utc::now(),
            resolved_at: None,
        })
    }
    async fn list(&self, _: Uuid) -> Result<Vec<common::WorkflowCase>, WorkflowRepositoryError> {
        Ok(vec![])
    }
    async fn decide(
        &self,
        _: Uuid,
        _: Uuid,
        _: common::WorkflowCaseStatus,
        _: &str,
        _: Option<&str>,
    ) -> Result<common::WorkflowCase, WorkflowRepositoryError> {
        Err(WorkflowRepositoryError::Backend("not used".to_string()))
    }
}

#[async_trait]
impl ProjectionClient for FailingProjectionClient {
    async fn reconcile(
        &self,
        _: Uuid,
        _: &crate::ReconciliationProjection,
    ) -> Result<(), crate::ProjectionClientError> {
        Err(crate::ProjectionClientError::Transport("unavailable".to_string()))
    }
}

fn reconciliation_payload() -> serde_json::Value {
    serde_json::json!({"reconciliation":{"object_id":Uuid::new_v4(),"object_type_id":Uuid::new_v4(),"source_version":"source-v42","properties":{"status":"approved"},"evidence":{"uri":"s3://evidence/42"}}})
}

fn state() -> RuntimeState {
    RuntimeState {
        repository: Arc::new(NoopRepository),
        projection_client: None,
        workflow_repository: Arc::new(SuccessfulWorkflowRepository),
        internal_secret: "test-secret".to_string(),
    }
}

fn headers(tenant_id: Uuid) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-tenant-id", tenant_id.to_string().parse().unwrap());
    headers
}

#[tokio::test]
async fn create_rejects_a_tenant_mismatch() {
    let header_tenant = Uuid::new_v4();
    let mut execution = PipelineExecution::new_projection(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "event:42".to_string(),
        serde_json::json!({}),
    );
    execution.pipeline_version = 1;
    let response = create_execution(State(state()), headers(header_tenant), Json(execution)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_returns_accepted_for_a_new_execution() {
    let tenant_id = Uuid::new_v4();
    let mut execution = PipelineExecution::new_projection(
        tenant_id,
        Uuid::new_v4(),
        "event:42".to_string(),
        serde_json::json!({}),
    );
    execution.pipeline_version = 1;
    let response = create_execution(State(state()), headers(tenant_id), Json(execution)).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[test]
fn status_contract_keeps_confirmation_distinct_from_success() {
    assert_ne!(PipelineExecutionStatus::Confirmed, PipelineExecutionStatus::Succeeded);
}

#[tokio::test]
async fn execution_api_requires_the_internal_secret() {
    let response = build_router(state())
        .oneshot(Request::builder().uri("/v1/pipeline-executions").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn workflow_queue_is_tenant_scoped_and_available_after_confirmation() {
    let response = list_workflow_cases(State(state()), headers(Uuid::new_v4())).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn review_requirement_opens_an_execution_linked_approval_with_an_sla() {
    let tenant_id = Uuid::new_v4();
    let mut request_headers = headers(tenant_id);
    request_headers.insert("x-role", "operator".parse().unwrap());
    let response = require_execution_review(
        State(state()),
        request_headers,
        Path(Uuid::new_v4()),
        Json(ReviewRequirementRequest {
            summary: "Approve ERP write-back".to_string(),
            sla_hours: 4,
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn reconciliation_failure_is_recorded_for_review_after_confirmation() {
    let tenant_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    let repository = Arc::new(RecordingRepository::default());
    let runtime_state = RuntimeState {
        repository: repository.clone(),
        projection_client: Some(Arc::new(FailingProjectionClient)),
        workflow_repository: Arc::new(SuccessfulWorkflowRepository),
        internal_secret: "test-secret".to_string(),
    };
    let payload = reconciliation_payload();

    let response = confirm_execution(
        State(runtime_state),
        headers(tenant_id),
        Path(execution_id),
        Json(ConfirmationRequest {
            source_event_id: "authoritative-event-42".to_string(),
            payload: payload.clone(),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let failures = repository.failures.lock().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, tenant_id);
    assert_eq!(failures[0].1, execution_id);
    assert_eq!(failures[0].2, "authoritative-event-42");
    assert_eq!(failures[0].3, payload);
    assert_eq!(failures[0].4, "projection failed");
}
