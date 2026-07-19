//! Action Executor (spec §6, service #7): consumes `event.created`, resolves the firing
//! TriggerDefinition's actions via Trigger Engine's API, dispatches each action (ADR-0007), and
//! writes an append-only ActionExecution audit row per action.

mod action_dispatcher;
mod execution_handlers;
mod execution_repository;
mod health;
mod process_event;
mod routing_action_dispatcher;
mod smtp_action_dispatcher;
mod trigger_client;

pub use action_dispatcher::{ActionDispatcher, DispatchError, HttpActionDispatcher};
pub use common::EVENT_CREATED_EXCHANGE;
pub use execution_handlers::{build_router as execution_router, list_executions, ExecutionState};
pub use execution_repository::{
    ExecutionRepository, ExecutionRepositoryError, PostgresExecutionRepository,
};
pub use health::build_router as health_router;
pub use process_event::{process_event, ActionDeps, ProcessError};
pub use routing_action_dispatcher::RoutingActionDispatcher;
pub use smtp_action_dispatcher::SmtpActionDispatcher;
pub use trigger_client::{HttpTriggerClient, TriggerClient, TriggerClientError};
