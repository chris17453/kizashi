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
    /// Optional alternate model/provider used once for transient primary-provider failures.
    /// Keeping this separate from the tenant's primary client makes the fallback explicit and
    /// prevents a failed model from causing the same request to loop forever.
    pub fallback_analysis_client: Option<Arc<dyn AnalysisClient>>,
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

fn resolve_fallback_client(
    deps: &AnalysisDeps,
    config: Option<&AnalysisConfig>,
) -> Option<Arc<dyn AnalysisClient>> {
    match config {
        // A tenant-selected compatible model can always fall back to the platform-default
        // Foundry client. This keeps the alternate path useful in local/dev deployments too;
        // operators only need ANALYSIS_FALLBACK_* when the platform default is the primary.
        Some(config) if config.provider == AnalysisProvider::OpenAiCompatible => {
            Some(deps.analysis_client.clone())
        }
        _ => deps.fallback_analysis_client.clone(),
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
    let fallback_analysis_client = resolve_fallback_client(deps, config.as_ref());

    let results = match analysis_client.analyze_batch(tenant_id, &records, prompt.as_deref()).await
    {
        Ok(results) => results,
        Err(primary_error)
            if primary_error.is_retryable() && fallback_analysis_client.is_some() =>
        {
            tracing::warn!(
                %tenant_id,
                error = %primary_error,
                "primary analysis provider failed transiently; trying fallback model"
            );
            fallback_analysis_client
                .as_ref()
                .expect("fallback checked above")
                .analyze_batch(tenant_id, &records, prompt.as_deref())
                .await
                .map_err(|fallback_error| {
                    BatchError::Analysis(format!(
                        "primary provider failed: {primary_error}; fallback provider failed: {fallback_error}"
                    ))
                })?
        }
        Err(error) => return Err(BatchError::Analysis(error.to_string())),
    };

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

/// Generates one bounded incident brief through the tenant's configured analysis provider.
/// Incident evidence is sent as a single synthetic record so the provider selection, fallback,
/// and response parsing remain identical to normal analysis calls.
pub async fn generate_incident_brief(
    deps: &AnalysisDeps,
    tenant_id: Uuid,
    evidence: serde_json::Value,
) -> Result<String, BatchError> {
    const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
    let serialized_evidence = serde_json::to_string(&evidence)
        .map_err(|error| BatchError::Analysis(error.to_string()))?;
    if serialized_evidence.len() > MAX_EVIDENCE_BYTES {
        return Err(BatchError::Analysis("incident evidence exceeds the 64 KiB limit".to_string()));
    }

    let config = deps
        .analysis_config_repository
        .get(tenant_id)
        .await
        .map_err(|error| BatchError::ConfigLookup(error.to_string()))?;
    let instructions = format!("Write a concise operational incident brief from the evidence below. State impact, scope, timeline, and the next investigation step. Do not invent facts. Return JSON with a single string field named text.\n\nEvidence:\n{serialized_evidence}");
    let prompt = config
        .as_ref()
        .filter(|config| !config.prompt.trim().is_empty())
        .map(|config| format!("Tenant analysis focus:\n{}\n\n{instructions}", config.prompt))
        .unwrap_or(instructions);
    let record = RawRecord {
        normalized_payload: Some(evidence),
        ..RawRecord::new(
            "incident-brief",
            common::SourceType::Generic,
            tenant_id,
            serde_json::json!({}),
        )
    };
    let primary = resolve_analysis_client(deps, config.as_ref());
    let fallback = resolve_fallback_client(deps, config.as_ref());
    let result = match primary.analyze_batch(tenant_id, std::slice::from_ref(&record), Some(&prompt)).await {
        Ok(mut results) => results.pop().ok_or_else(|| {
            BatchError::Analysis("analysis provider returned no incident brief".to_string())
        })?,
        Err(primary_error) if primary_error.is_retryable() && fallback.is_some() => fallback
            .as_ref()
            .expect("fallback checked above")
            .analyze_batch(tenant_id, &[record], Some(&prompt))
            .await
            .map_err(|fallback_error| {
                BatchError::Analysis(format!(
                    "primary provider failed: {primary_error}; fallback provider failed: {fallback_error}"
                ))
            })?
            .into_iter()
            .next()
            .ok_or_else(|| BatchError::Analysis("analysis provider returned no incident brief".to_string()))?,
        Err(error) => return Err(BatchError::Analysis(error.to_string())),
    };

    let text = result
        .get("text")
        .or_else(|| result.get("summary"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .or_else(|| result.as_str().map(str::to_string))
        .ok_or_else(|| {
            BatchError::Analysis("analysis provider returned no brief text".to_string())
        })?;
    if text.len() > 8 * 1024 {
        return Err(BatchError::Analysis(
            "analysis provider returned an oversized brief".to_string(),
        ));
    }
    Ok(text)
}
