//! Ingestion Service (spec §6, service #3): validates and persists RawRecords to the hot
//! store, then publishes `record.ingested` to the message bus for Normalization Service.

mod event_publisher;
mod health;
mod ingest_handler;
mod raw_record_repository;

pub use event_publisher::{
    EventPublisher, PublishError, RabbitMqEventPublisher, RECORD_INGESTED_EXCHANGE,
};
pub use ingest_handler::{ingest_record, IngestState, NewRawRecordRequest};
pub use raw_record_repository::{
    PostgresRawRecordRepository, RawRecordRepository, RepositoryError,
};

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: IngestState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/records", post(ingest_record))
        .with_state(state)
}
