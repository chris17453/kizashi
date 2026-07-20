//! Trigger Engine (spec §6, service #6): consumes `record.analyzed`, classifies candidate
//! event types (ADR-0006), evaluates enabled TriggerDefinitions against a rolling window per
//! (tenant, event_type, group_key), writes firing Events to ClickHouse, and publishes
//! `event.created`.

mod api;
mod classify;
mod event_publisher;
mod event_store;
mod health;
mod internal_secret;
mod process_analyzed_record;
mod retry;
mod signal_repository;
mod trigger_repository;

pub use api::{build_router as api_router, ApiState};
pub use classify::{candidates, group_key, Candidate};
pub use common::{EVENT_CREATED_EXCHANGE, RECORD_ANALYZED_EXCHANGE, TRIGGER_CHANGED_EXCHANGE};
pub use event_publisher::{EventPublisher, PublishError, RabbitMqEventPublisher};
pub use event_store::{ClickHouseEventStore, EventStore, EventStoreError};
pub use health::build_router as health_router;
pub use process_analyzed_record::{
    evaluate_trigger, process_analyzed_record, ProcessError, TriggerDeps,
};
pub use retry::{
    retry_count, should_dead_letter, with_incremented_retry_count, MAX_RETRIES, RETRY_COUNT_HEADER,
};
pub use signal_repository::{
    AnalyzedSignal, PostgresSignalRepository, SignalRepository, SignalRepositoryError,
};
pub use trigger_repository::{
    PostgresTriggerRepository, TriggerRepository, TriggerRepositoryError,
};
