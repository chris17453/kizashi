//! Normalization Service (spec §6, service #4): consumes `record.ingested`, applies the
//! tenant's NormalizationMapping, writes normalized_payload back through Ingestion Service's
//! API, and publishes `record.normalized`.

mod event_publisher;
mod health;
mod mapping_repository;
mod process_normalization;
mod record_client;
mod retry;

pub use common::{MAPPING_CHANGED_EXCHANGE, RECORD_INGESTED_EXCHANGE, RECORD_NORMALIZED_EXCHANGE};
pub use event_publisher::{EventPublisher, PublishError, RabbitMqEventPublisher};
pub use health::build_router as health_router;
pub use mapping_repository::{
    MappingRepository, MappingRepositoryError, PostgresMappingRepository,
};
pub use process_normalization::{
    process_normalization, source_type_key, NormalizationDeps, ProcessError, ProcessOutcome,
};
pub use record_client::{HttpRecordClient, RecordClient, RecordClientError};
pub use retry::{
    retry_count, should_dead_letter, with_incremented_retry_count, MAX_RETRIES, RETRY_COUNT_HEADER,
};
