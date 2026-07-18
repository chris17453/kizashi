//! Ingestion Service (spec §6, service #3): validates and persists RawRecords to the hot
//! store, then publishes `record.ingested` to the message bus for Normalization Service.

mod event_publisher;
mod health;
mod ingest_handler;
mod raw_record_repository;
mod update_normalized_handler;

pub use common::RECORD_INGESTED_EXCHANGE;
pub use event_publisher::{EventPublisher, PublishError, RabbitMqEventPublisher};
pub use ingest_handler::{ingest_record, IngestError, IngestState, NewRawRecordRequest};
pub use raw_record_repository::{
    PostgresRawRecordRepository, RawRecordRepository, RepositoryError,
};
pub use update_normalized_handler::{update_normalized_payload, UpdateNormalizedPayloadRequest};

use axum::routing::{get, patch, post};
use axum::Router;

pub fn build_router(state: IngestState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/records", post(ingest_record))
        .route("/v1/records/:id/normalized", patch(update_normalized_payload))
        .with_state(state)
}
