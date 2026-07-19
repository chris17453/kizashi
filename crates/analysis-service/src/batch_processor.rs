#[path = "batch_processor_test.rs"]
#[cfg(test)]
mod batch_processor_test;

use crate::analysis_client::{AnalysisClient, OpenAiCompatibleAnalysisClient};
use crate::analysis_config_repository::AnalysisConfigRepository;
use crate::event_publisher::EventPublisher;
use common::{AnalysisConfig, AnalysisProvider, AnalyzedRecord, RawRecord};
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
    /// The platform-wide default client (Foundry), used for tenants with no config or
    /// `AnalysisProvider::AzureFoundry` (ADR-0031).
    pub analysis_client: Arc<dyn AnalysisClient>,
    pub publisher: Arc<dyn EventPublisher>,
    pub analysis_config_repository: Arc<dyn AnalysisConfigRepository>,
    /// Reused to build per-tenant `OpenAiCompatibleAnalysisClient`s so each call doesn't pay
    /// for a fresh connection pool.
    pub http_client: reqwest::Client,
    /// How many `OpenAiCompatibleAnalysisClient` requests run concurrently per batch
    /// (ADR-0035) — a slow reasoning model turns a real multi-hundred-record backlog into a
    /// multi-hour serial queue at concurrency 1 (observed live), so this is a real operator
    /// knob, not a hardcoded constant.
    pub openai_compatible_concurrency: usize,
}

/// Picks the client for a tenant's configured provider. Resolved per call, not cached, so a
/// credential/endpoint change in `analysis_configs` takes effect on the very next batch (ADR-0031).
fn resolve_analysis_client(
    deps: &AnalysisDeps,
    config: Option<&AnalysisConfig>,
) -> Arc<dyn AnalysisClient> {
    match config {
        Some(config) if config.provider == AnalysisProvider::OpenAiCompatible => Arc::new(
            OpenAiCompatibleAnalysisClient::new(
                deps.http_client.clone(),
                config.endpoint.clone().unwrap_or_default(),
                config.api_key.clone(),
                config.model.clone().unwrap_or_default(),
            )
            .with_concurrency(deps.openai_compatible_concurrency),
        ),
        _ => deps.analysis_client.clone(),
    }
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

    let config = deps
        .analysis_config_repository
        .get(tenant_id)
        .await
        .map_err(|e| BatchError::ConfigLookup(e.to_string()))?;
    let prompt = config.as_ref().map(|c| c.prompt.clone());
    let analysis_client = resolve_analysis_client(deps, config.as_ref());

    let results = analysis_client
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
