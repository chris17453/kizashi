#[path = "reimport_test.rs"]
#[cfg(test)]
mod reimport_test;

use crate::archive_store::{ArchiveStore, ArchiveStoreError};
use crate::raw_record_client::RawRecordClient;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone)]
pub struct ReimportState {
    pub archive_store: Arc<dyn ArchiveStore>,
    pub record_client: Arc<dyn RawRecordClient>,
}

#[derive(Debug, Error)]
pub enum ReimportError {
    #[error("failed to read archive batch: {0}")]
    Archive(#[from] ArchiveStoreError),
}

#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct ReimportSummary {
    pub records_reimported: usize,
    pub records_failed: usize,
}

/// Reads an archived batch and re-feeds every record through Ingestion Service's normal ingest
/// path (spec §9 "reimport"; ADR-0011 point 4 on why this bypasses Ingestion Gateway). A
/// per-record reimport failure is logged and counted, not fatal to the batch — one bad record
/// shouldn't block reimporting the rest.
pub async fn reimport(
    state: &ReimportState,
    archive_key: &str,
) -> Result<ReimportSummary, ReimportError> {
    let (_manifest, records) = state.archive_store.read_batch(archive_key).await?;

    let mut summary = ReimportSummary::default();
    for record in &records {
        match state.record_client.reimport(record).await {
            Ok(()) => summary.records_reimported += 1,
            Err(e) => {
                tracing::error!(record_id = %record.id, error = %e, "failed to reimport record");
                summary.records_failed += 1;
            }
        }
    }
    Ok(summary)
}
