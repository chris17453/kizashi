#[path = "analysis_config_test.rs"]
#[cfg(test)]
mod analysis_config_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A tenant's free-text description of what Analysis Service should ask the AI/ML backend to
/// look for (ADR-0019) — one row per tenant, since Foundry/ML calls never mix tenants in one
/// batch (ADR-0004). Empty/missing means "no prompt configured", not "empty string is a valid
/// instruction" — callers treat `None` from a repository lookup as today's existing global
/// behavior, not an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub tenant_id: Uuid,
    pub prompt: String,
    pub updated_at: DateTime<Utc>,
}

impl AnalysisConfig {
    pub fn new(tenant_id: Uuid, prompt: impl Into<String>) -> Self {
        Self { tenant_id, prompt: prompt.into(), updated_at: Utc::now() }
    }
}
