mod command_adapter;
mod execution_repository;
mod handlers;
mod health;
mod internal_secret;
mod ontology_projection_client;
mod outbox_publisher;
mod outbox_repository;
mod reconciliation;
mod retry;
mod workflow_repository;

pub use command_adapter::{
    CommandAdapter, CommandDispatch, CommandDispatchError, HttpCommandAdapter,
};
pub use execution_repository::{
    ConfirmationResult, ExecutionRepository, ExecutionRepositoryError, ExecutionRequestResult,
    PostgresExecutionRepository,
};
pub use handlers::{build_router, confirm_execution, create_execution, RuntimeState};
pub use health::healthz;
pub use ontology_projection_client::{
    HttpProjectionClient, ProjectionClient, ProjectionClientError,
};
pub use outbox_publisher::publish_once;
pub use outbox_repository::{
    OutboxMessage, OutboxRepository, OutboxRepositoryError, PostgresOutboxRepository,
};
pub use reconciliation::{parse_projection, ReconciliationProjection};
pub use retry::{should_dead_letter, MAX_OUTBOX_PUBLISH_ATTEMPTS};
pub use workflow_repository::{
    PostgresWorkflowRepository, WorkflowRepository, WorkflowRepositoryError,
};
