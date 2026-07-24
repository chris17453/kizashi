#[path = "sweep_test.rs"]
#[cfg(test)]
mod sweep_test;

use crate::archive_store::ArchiveStore;
use crate::compliance_hold::ComplianceHoldRepository;
use crate::raw_record_client::RawRecordClient;
use crate::retention_policy::{DataClass, RetentionPolicyRepository};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone)]
pub struct SweepState {
    pub policy_repository: Arc<dyn RetentionPolicyRepository>,
    pub record_client: Arc<dyn RawRecordClient>,
    pub archive_store: Arc<dyn ArchiveStore>,
    pub hold_repository: Option<Arc<dyn ComplianceHoldRepository>>,
}

#[derive(Debug, Error)]
pub enum SweepError {
    #[error("failed to list retention policies: {0}")]
    PolicyList(String),
}

#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct SweepSummary {
    pub records_archived: usize,
    pub batches_written: Vec<String>,
}

/// Enforces every enabled `Raw`-data-class retention policy (ADR-0011: v1 only enforces `Raw`)
/// by archiving records older than the policy's TTL, then deleting them from the hot store —
/// archive-then-delete, never the reverse, so a crash between the two steps only ever risks a
/// record being re-archived on the next sweep, never lost. `now` is an explicit parameter so
/// this is unit-testable without wall-clock coupling (CLAUDE.md §2).
pub async fn sweep(
    state: &SweepState,
    now: DateTime<Utc>,
    batch_limit: i64,
) -> Result<SweepSummary, SweepError> {
    let policies = state
        .policy_repository
        .list_all_enabled()
        .await
        .map_err(|e| SweepError::PolicyList(e.to_string()))?;

    let mut summary = SweepSummary::default();

    for policy in policies.into_iter().filter(|p| p.data_class == DataClass::Raw) {
        if let Some(holds) = &state.hold_repository {
            if holds
                .has_active(policy.tenant_id, policy.data_class)
                .await
                .map_err(|e| SweepError::PolicyList(e.to_string()))?
            {
                tracing::info!(tenant_id = %policy.tenant_id, data_class = ?policy.data_class, "retention sweep skipped by active compliance hold");
                continue;
            }
        }
        let cutoff = now - chrono::Duration::days(policy.ttl_days as i64);
        let window_start = cutoff - chrono::Duration::days(policy.ttl_days as i64);

        loop {
            let batch = match state
                .record_client
                .list_older_than(policy.tenant_id, cutoff, batch_limit)
                .await
            {
                Ok(batch) => batch,
                Err(e) => {
                    tracing::error!(tenant_id = %policy.tenant_id, error = %e, "failed to list records for sweep");
                    break;
                }
            };
            if batch.is_empty() {
                break;
            }
            let batch_len = batch.len();

            let key = match state
                .archive_store
                .write_batch(policy.tenant_id, "raw", &batch, window_start, cutoff)
                .await
            {
                Ok(key) => key,
                Err(e) => {
                    tracing::error!(tenant_id = %policy.tenant_id, error = %e, "failed to archive sweep batch, skipping delete");
                    break;
                }
            };
            summary.batches_written.push(key);

            for record in &batch {
                if let Err(e) = state.record_client.delete(policy.tenant_id, record.id).await {
                    tracing::error!(record_id = %record.id, error = %e, "failed to delete archived record, will re-archive next sweep");
                } else {
                    summary.records_archived += 1;
                }
            }

            if batch_len < batch_limit as usize {
                break;
            }
        }
    }

    Ok(summary)
}
