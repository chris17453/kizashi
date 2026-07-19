#[path = "batch_processor_test.rs"]
#[cfg(test)]
mod batch_processor_test;

use crate::analysis_client::AnalysisClient;
use crate::analysis_config_repository::AnalysisConfigRepository;
use crate::event_publisher::EventPublisher;
use common::{AnalyzedRecord, RawRecord};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("analysis call failed: {0}")]
    Analysis(String),
    #[error("failed to read analysis config: {0}")]
    ConfigLookup(String),
}

#[derive(Clone)]
pub struct AnalysisDeps {
    pub analysis_client: Arc<dyn AnalysisClient>,
    pub publisher: Arc<dyn EventPublisher>,
    pub analysis_config_repository: Arc<dyn AnalysisConfigRepository>,
}

/// Splits a mixed-tenant batch of consumed messages into per-tenant groups, preserving
/// arrival order within each group. Analysis calls never mix tenants in one Foundry/ML
/// invocation (ADR-0004), no matter how the consumer happened to interleave deliveries.
pub fn group_by_tenant(records: Vec<RawRecord>) -> BTreeMap<Uuid, Vec<RawRecord>> {
    let mut groups: BTreeMap<Uuid, Vec<RawRecord>> = BTreeMap::new();
    for record in records {
        groups.entry(record.tenant_id).or_default().push(record);
    }
    groups
}

/// Calls Foundry/ML once for a single tenant's batch, then publishes one `record.analyzed`
/// per input record. A publish failure for one record is logged and does not stop the rest of
/// the batch from being published — same "durable write already happened, don't fail the
/// whole batch over a downstream notification" policy as Ingestion/Normalization Service.
pub async fn process_batch(
    deps: &AnalysisDeps,
    tenant_id: Uuid,
    records: Vec<RawRecord>,
) -> Result<usize, BatchError> {
    if records.is_empty() {
        return Ok(0);
    }

    let prompt = deps
        .analysis_config_repository
        .get(tenant_id)
        .await
        .map_err(|e| BatchError::ConfigLookup(e.to_string()))?
        .map(|c| c.prompt);

    let results = deps
        .analysis_client
        .analyze_batch(tenant_id, &records, prompt.as_deref())
        .await
        .map_err(|e| BatchError::Analysis(e.to_string()))?;

    let mut published = 0;
    for (record, analysis) in records.into_iter().zip(results) {
        let analyzed = AnalyzedRecord::new(record, analysis);
        match deps.publisher.publish_record_analyzed(&analyzed).await {
            Ok(()) => published += 1,
            Err(e) => {
                tracing::error!(record_id = %analyzed.record.id, error = %e, "failed to publish record.analyzed");
            }
        }
    }
    Ok(published)
}
