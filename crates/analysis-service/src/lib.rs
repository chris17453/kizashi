//! Analysis Service (spec §6, service #5): consumes `record.normalized`, calls Azure AI
//! Foundry/ML in per-tenant micro-batches (ADR-0004), and publishes `record.analyzed`.

mod analysis_client;
mod analysis_config_repository;
mod batch_processor;
mod event_publisher;
mod health;
mod retry;

pub use analysis_client::{
    AnalysisClient, AnalysisError, FoundryAnalysisClient, OpenAiCompatibleAnalysisClient,
};
pub use analysis_config_repository::{
    AnalysisConfigRepository, AnalysisConfigRepositoryError, PostgresAnalysisConfigRepository,
};
pub use batch_processor::{group_by_tenant, process_batch, AnalysisDeps, BatchError};
pub use common::{
    ANALYSIS_CONFIG_CHANGED_EXCHANGE, RECORD_ANALYZED_EXCHANGE, RECORD_NORMALIZED_EXCHANGE,
};
pub use event_publisher::{EventPublisher, PublishError, RabbitMqEventPublisher};
pub use health::{build_router as health_router, ConsumerHeartbeat};
pub use retry::{
    retry_count, should_dead_letter, with_incremented_retry_count, MAX_RETRIES, RETRY_COUNT_HEADER,
};
