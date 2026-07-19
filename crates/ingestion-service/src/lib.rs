//! Ingestion Service (spec §6, service #3): validates and persists RawRecords to the hot
//! store, then publishes `record.ingested` to the message bus for Normalization Service.

mod connector_stats_handler;
mod delete_record_handler;
mod event_publisher;
mod health;
mod ingest_handler;
mod list_records_handler;
mod raw_record_repository;
mod record_search_handler;
mod update_normalized_handler;

pub use common::RECORD_INGESTED_EXCHANGE;
pub use connector_stats_handler::{
    get_connector_stats, list_records_by_connector, ListByConnectorQuery,
};
pub use delete_record_handler::delete_record;
pub use event_publisher::{EventPublisher, PublishError, RabbitMqEventPublisher};
pub use ingest_handler::{
    ingest_record, reprocess_records, IngestError, IngestState, NewRawRecordRequest,
    ReprocessResponse,
};
pub use list_records_handler::{list_records, ListRecordsQuery};
pub use raw_record_repository::{
    ConnectorStats, PostgresRawRecordRepository, RawRecordRepository, RecordSearchFilter,
    RepositoryError,
};
pub use record_search_handler::{get_record, search_records, SearchRecordsQuery};
pub use update_normalized_handler::{update_normalized_payload, UpdateNormalizedPayloadRequest};

use axum::routing::{get, patch, post};
use axum::Router;

pub fn build_router(state: IngestState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/records", post(ingest_record).get(list_records))
        .route("/v1/records/reprocess", post(reprocess_records))
        .route("/v1/records/stats", get(get_connector_stats))
        .route("/v1/records/by-connector", get(list_records_by_connector))
        .route("/v1/records/search", get(search_records))
        .route("/v1/records/:id/normalized", patch(update_normalized_payload))
        .route("/v1/records/:id", get(get_record).delete(delete_record))
        .with_state(state)
}
