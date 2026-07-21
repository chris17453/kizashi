#[path = "process_normalization_test.rs"]
#[cfg(test)]
mod process_normalization_test;

use crate::event_publisher::EventPublisher;
use crate::fingerprint::compute_fingerprint;
use crate::fingerprint_repository::{DedupOutcome, FingerprintRepository};
use crate::mapping_repository::MappingRepository;
use crate::record_client::RecordClient;
use common::RawRecord;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("mapping lookup failed: {0}")]
    MappingLookup(String),
    #[error("failed to write normalized_payload back to ingestion-service: {0}")]
    RecordUpdate(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    Normalized,
    /// No NormalizationMapping exists yet for this tenant/source_type. Not an error — an
    /// operator hasn't configured this source type yet — so the message is still acked rather
    /// than redelivered forever.
    NoMappingConfigured,
    /// An exact duplicate within the mapping's dedup window (ADR-0112) — normalized_payload
    /// was still written back (raw/normalized data is never silently dropped), but
    /// `record.normalized` was not published, so analysis-service/trigger-engine never
    /// re-react to a repeat.
    Suppressed,
}

#[derive(Clone)]
pub struct NormalizationDeps {
    pub mapping_repository: Arc<dyn MappingRepository>,
    pub record_client: Arc<dyn RecordClient>,
    pub publisher: Arc<dyn EventPublisher>,
    pub fingerprint_repository: Arc<dyn FingerprintRepository>,
}

/// Converts a common::SourceType into the string key NormalizationMapping rows are keyed by
/// (e.g. `SourceType::Ticket` -> `"ticket"`), matching the enum's own snake_case serde repr.
pub fn source_type_key(source_type: common::SourceType) -> String {
    serde_json::to_string(&source_type).unwrap_or_default().trim_matches('"').to_string()
}

/// Core normalization step for one consumed `record.ingested` message: look up the tenant's
/// mapping for this source type, apply it, write the result back through Ingestion Service's
/// API (never its database directly — spec §2 principle 1), then publish `record.normalized`.
/// A publish failure is logged, not propagated — the normalized_payload write already
/// succeeded and is the durable, replayable source of truth (same policy as Ingestion
/// Service's `record.ingested` publish).
pub async fn process_normalization(
    deps: &NormalizationDeps,
    record: &RawRecord,
) -> Result<ProcessOutcome, ProcessError> {
    let source_type_key = source_type_key(record.source_type);

    let mapping = deps
        .mapping_repository
        .active_mapping(record.tenant_id, &source_type_key)
        .await
        .map_err(|e| ProcessError::MappingLookup(e.to_string()))?;

    let Some(mapping) = mapping else {
        tracing::warn!(
            record_id = %record.id,
            tenant_id = %record.tenant_id,
            source_type = %source_type_key,
            "no normalization mapping configured; skipping"
        );
        return Ok(ProcessOutcome::NoMappingConfigured);
    };

    let normalized_payload = mapping.apply(&record.raw_payload);

    deps.record_client
        .update_normalized_payload(record.id, &normalized_payload)
        .await
        .map_err(|e| ProcessError::RecordUpdate(e.to_string()))?;

    // ADR-0112: check for an exact duplicate before publishing. A fingerprint-store failure
    // fails open (treated as a fresh occurrence, logged not propagated) — dedup is a
    // noise-reduction layer, not a correctness guarantee, so a transient dedup-store hiccup
    // must never cause a genuine record.normalized to go unpublished.
    if let Some(fingerprint) = compute_fingerprint(&mapping.dedup_fields, &normalized_payload) {
        match deps
            .fingerprint_repository
            .check_and_record(
                record.tenant_id,
                &fingerprint,
                record.id,
                mapping.dedup_window_seconds,
            )
            .await
        {
            Ok(DedupOutcome::Duplicate) => {
                tracing::info!(record_id = %record.id, tenant_id = %record.tenant_id, "suppressing exact-duplicate record.normalized");
                return Ok(ProcessOutcome::Suppressed);
            }
            Ok(DedupOutcome::New) => {}
            Err(e) => {
                tracing::error!(record_id = %record.id, error = %e, "fingerprint check failed, publishing anyway");
            }
        }
    }

    let mut updated_record = record.clone();
    updated_record.normalized_payload = Some(normalized_payload);

    if let Err(e) = deps.publisher.publish_record_normalized(&updated_record).await {
        tracing::error!(record_id = %record.id, error = %e, "failed to publish record.normalized");
    }

    Ok(ProcessOutcome::Normalized)
}
