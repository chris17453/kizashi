#[path = "analysis_client_test.rs"]
#[cfg(test)]
pub(crate) mod analysis_client_test;

use async_trait::async_trait;
use common::RawRecord;
use futures_util::StreamExt;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("AI/ML backend unreachable: {0}")]
    Unreachable(String),
    #[error("AI/ML backend rejected the batch: HTTP {0}")]
    Rejected(u16),
    #[error("AI/ML backend returned {got} results for a batch of {expected}")]
    ResultCountMismatch { expected: usize, got: usize },
}

/// Calls Azure AI Foundry/ML for a tenant-homogeneous batch of records (ADR-0004: analysis is
/// invoked in micro-batches, never mixing tenants in one call). Returns exactly one analysis
/// result per input record, in the same order, so callers can zip results back onto records
/// without needing a correlation id round-trip. `prompt` is the tenant's optional AI analysis
/// prompt (ADR-0019) — `None` means today's existing global-analysis behavior, unchanged.
#[async_trait]
pub trait AnalysisClient: Send + Sync {
    async fn analyze_batch(
        &self,
        tenant_id: Uuid,
        records: &[RawRecord],
        prompt: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, AnalysisError>;
}

pub struct FoundryAnalysisClient {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl FoundryAnalysisClient {
    pub fn new(client: reqwest::Client, endpoint: String, api_key: String) -> Self {
        Self { client, endpoint, api_key }
    }
}

#[async_trait]
impl AnalysisClient for FoundryAnalysisClient {
    async fn analyze_batch(
        &self,
        tenant_id: Uuid,
        records: &[RawRecord],
        prompt: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        let payloads: Vec<&serde_json::Value> = records
            .iter()
            .map(|r| r.normalized_payload.as_ref().unwrap_or(&r.raw_payload))
            .collect();

        let mut body = serde_json::json!({"tenant_id": tenant_id, "inputs": payloads});
        if let Some(prompt) = prompt {
            body["prompt"] = serde_json::Value::String(prompt.to_string());
        }

        let response = self
            .client
            .post(&self.endpoint)
            .header("api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AnalysisError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct FoundryResponse {
            results: Vec<serde_json::Value>,
        }
        let body: FoundryResponse =
            response.json().await.map_err(|e| AnalysisError::Unreachable(e.to_string()))?;

        if body.results.len() != records.len() {
            return Err(AnalysisError::ResultCountMismatch {
                expected: records.len(),
                got: body.results.len(),
            });
        }
        Ok(body.results)
    }
}

/// Calls any provider speaking the standard `/v1/chat/completions` shape — Ollama, OpenAI, and
/// Azure OpenAI in "compatible mode" all fit, so one client covers all three (ADR-0031). Unlike
/// `FoundryAnalysisClient`'s single batched call, chat-completions isn't a batch API: asking a
/// chat model to return N JSON results reliably in one response is unreliable, so this client
/// makes one request per record instead — up to `concurrency` of them in flight at once
/// (ADR-0035), not strictly one-at-a-time: a slow reasoning model turns a real multi-hundred-
/// record backlog into a multi-hour serial queue otherwise (observed live). `api_key` is
/// optional since a local Ollama instance needs no credential at all.
pub struct OpenAiCompatibleAnalysisClient {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
    model: String,
    concurrency: usize,
}

const DEFAULT_CONCURRENCY: usize = 4;

impl OpenAiCompatibleAnalysisClient {
    pub fn new(
        client: reqwest::Client,
        endpoint: String,
        api_key: Option<String>,
        model: String,
    ) -> Self {
        Self { client, endpoint, api_key, model, concurrency: DEFAULT_CONCURRENCY }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.max(1);
        self
    }

    async fn analyze_one(
        &self,
        payload: &serde_json::Value,
        prompt: Option<&str>,
    ) -> Result<serde_json::Value, AnalysisError> {
        let content = match prompt {
            Some(prompt) => format!("{prompt}\n\n{payload}"),
            None => payload.to_string(),
        };
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": content}],
        });

        let url = format!("{}/chat/completions", self.endpoint.trim_end_matches('/'));
        let mut request = self.client.post(&url).json(&body);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response =
            request.send().await.map_err(|e| AnalysisError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AnalysisError::Rejected(response.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        struct ChatCompletionResponse {
            choices: Vec<ChatCompletionChoice>,
        }
        #[derive(serde::Deserialize)]
        struct ChatCompletionChoice {
            message: ChatCompletionMessage,
        }
        #[derive(serde::Deserialize)]
        struct ChatCompletionMessage {
            content: String,
        }

        let parsed: ChatCompletionResponse =
            response.json().await.map_err(|e| AnalysisError::Unreachable(e.to_string()))?;
        let content =
            parsed.choices.into_iter().next().map(|c| c.message.content).unwrap_or_default();

        Ok(serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({"text": content})))
    }
}

#[async_trait]
impl AnalysisClient for OpenAiCompatibleAnalysisClient {
    async fn analyze_batch(
        &self,
        _tenant_id: Uuid,
        records: &[RawRecord],
        prompt: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        // Payloads are cloned up front so each future owns its input (avoids a higher-ranked
        // lifetime the compiler otherwise can't unify across the closure boundary). `buffered`
        // (not `buffer_unordered`) runs up to `concurrency` requests in flight at once while
        // yielding results in the same order the futures were created — the ordering callers
        // depend on (results[i] corresponds to records[i]) comes for free, no re-sort needed.
        let payloads: Vec<serde_json::Value> = records
            .iter()
            .map(|r| r.normalized_payload.clone().unwrap_or_else(|| r.raw_payload.clone()))
            .collect();
        let prompt_owned = prompt.map(str::to_string);

        futures_util::stream::iter(payloads.into_iter().map(|payload| {
            let prompt_ref = prompt_owned.as_deref();
            async move { self.analyze_one(&payload, prompt_ref).await }
        }))
        .buffered(self.concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect()
    }
}
