#[path = "analysis_config_test.rs"]
#[cfg(test)]
mod analysis_config_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which `AnalysisClient` implementation a tenant's batch is analyzed with (ADR-0031).
/// `AzureFoundry` is the default (unset/missing in storage deserializes to this via `Default`)
/// — today's existing platform-wide behavior, unchanged for every tenant that never opts in to
/// a different provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnalysisProvider {
    #[default]
    #[serde(rename = "azure_foundry")]
    AzureFoundry,
    /// The standard `/v1/chat/completions` shape — what Ollama, OpenAI, and Azure OpenAI (in
    /// OpenAI-compatible mode) all actually implement, so one client covers all three.
    /// Renamed explicitly (not `rename_all = "snake_case"`) because that would produce
    /// `open_ai_compatible` — "Ai" splits into its own word — which doesn't match the
    /// `openai_compatible` string this crate's Postgres repositories store in the `provider`
    /// column. Wire format and storage format must agree.
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

/// A tenant's AI analysis configuration (ADR-0019, extended by ADR-0031): what to ask the
/// AI/ML backend to look for, and — as of ADR-0031 — which provider/model/endpoint to ask it
/// with. One row per tenant, since Foundry/ML calls never mix tenants in one batch (ADR-0004).
/// `prompt` empty/missing means "no prompt configured", not "empty string is a valid
/// instruction" — callers treat `None` from a repository lookup as today's existing global
/// behavior, not an error. `model`/`endpoint`/`api_key` are only meaningful for
/// `OpenAiCompatible`; `AzureFoundry` always uses the platform-wide env-configured client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub tenant_id: Uuid,
    pub prompt: String,
    #[serde(default)]
    pub provider: AnalysisProvider,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl AnalysisConfig {
    pub fn new(tenant_id: Uuid, prompt: impl Into<String>) -> Self {
        Self {
            tenant_id,
            prompt: prompt.into(),
            provider: AnalysisProvider::default(),
            model: None,
            endpoint: None,
            api_key: None,
            updated_at: Utc::now(),
        }
    }
}
