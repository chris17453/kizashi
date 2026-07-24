#[path = "execution_client_test.rs"]
#[cfg(test)]
pub(crate) mod execution_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ActionExecutionSummary {
    pub id: Uuid,
    pub trigger_id: Uuid,
    pub event_id: Uuid,
    pub action_type: String,
    pub status: String,
    pub executed_at: DateTime<Utc>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeadLetterQueueSummary {
    pub service: String,
    pub count: Option<u32>,
    pub has_messages: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisResilienceSummary {
    pub consumer_alive: bool,
    pub fallback_configured: bool,
    pub dead_letter_count: u32,
}

#[derive(Debug, Error)]
pub enum ExecutionClientError {
    #[error("action executor unreachable: {0}")]
    Unreachable(String),
    #[error("action executor rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads action executions from Action Executor's own admin endpoint (its first read/query
/// surface, ADR-0017 follow-up) — direct-call trust boundary like `TriggersClient`/
/// `AgentsClient` (no gateway sits in front of Action Executor either).
#[async_trait]
pub trait ExecutionClient: Send + Sync {
    async fn list_executions_for_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecutionSummary>, ExecutionClientError>;

    /// Returns the number of action messages that exhausted their retry budget and are waiting
    /// for operator recovery. Implementations that do not expose the operational queue keep the
    /// UI safe with an unknown-as-zero default; the HTTP client uses Action Executor's
    /// internal-secret-protected endpoint.
    async fn dead_letter_count(&self) -> Result<u32, ExecutionClientError> {
        Ok(0)
    }

    /// Replays one oldest dead-lettered action message with a fresh retry budget.
    async fn replay_dead_letter(&self) -> Result<bool, ExecutionClientError> {
        Ok(false)
    }

    async fn dead_letter_queues(
        &self,
    ) -> Result<Vec<DeadLetterQueueSummary>, ExecutionClientError> {
        Ok(Vec::new())
    }

    async fn replay_dead_letter_queue(&self, _service: &str) -> Result<bool, ExecutionClientError> {
        Ok(false)
    }

    /// Reads non-secret analysis-provider posture when the analysis service exposes it.
    /// Implementations without that endpoint return `None` so older deployments remain usable.
    async fn analysis_resilience(
        &self,
    ) -> Result<Option<AnalysisResilienceSummary>, ExecutionClientError> {
        Ok(None)
    }
}

pub struct HttpExecutionClient {
    client: reqwest::Client,
    action_executor_url: String,
    dead_letter_services: Vec<(String, String)>,
}

impl HttpExecutionClient {
    pub fn new(client: reqwest::Client, action_executor_url: String) -> Self {
        Self {
            client,
            action_executor_url: action_executor_url.clone(),
            dead_letter_services: vec![("action-executor".to_string(), action_executor_url)],
        }
    }

    pub fn with_dead_letter_services(mut self, services: Vec<(&str, String)>) -> Self {
        for (service, url) in services {
            if !self.dead_letter_services.iter().any(|(existing, _)| existing == service) {
                self.dead_letter_services.push((service.to_string(), url));
            }
        }
        self
    }
}

#[async_trait]
impl ExecutionClient for HttpExecutionClient {
    async fn list_executions_for_event(
        &self,
        tenant_id: Uuid,
        event_id: Uuid,
    ) -> Result<Vec<ActionExecutionSummary>, ExecutionClientError> {
        let response = self
            .client
            .get(format!("{}/v1/action-executions", self.action_executor_url))
            .query(&[("event_id", event_id.to_string())])
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ExecutionClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| ExecutionClientError::Unreachable(e.to_string()))
    }

    async fn dead_letter_count(&self) -> Result<u32, ExecutionClientError> {
        #[derive(serde::Deserialize)]
        struct CountResponse {
            count: u32,
        }
        let response = self
            .client
            .get(format!("{}/v1/dead-letter", self.action_executor_url))
            .send()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(ExecutionClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))
            .map(|body: CountResponse| body.count)
    }

    async fn replay_dead_letter(&self) -> Result<bool, ExecutionClientError> {
        #[derive(serde::Deserialize)]
        struct ReplayResponse {
            replayed: bool,
        }
        let response = self
            .client
            .post(format!("{}/v1/dead-letter/replay", self.action_executor_url))
            .send()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(ExecutionClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))
            .map(|body: ReplayResponse| body.replayed)
    }

    async fn dead_letter_queues(
        &self,
    ) -> Result<Vec<DeadLetterQueueSummary>, ExecutionClientError> {
        #[derive(serde::Deserialize)]
        struct CountResponse {
            count: u32,
        }
        let mut queues = Vec::with_capacity(self.dead_letter_services.len());
        for (service, url) in &self.dead_letter_services {
            let count = match self.client.get(format!("{url}/v1/dead-letter")).send().await {
                Ok(response) if response.status().is_success() => {
                    response.json::<CountResponse>().await.ok().map(|body| body.count)
                }
                _ => None,
            };
            let has_messages = count.is_some_and(|count| count > 0);
            queues.push(DeadLetterQueueSummary { service: service.clone(), count, has_messages });
        }
        Ok(queues)
    }

    async fn replay_dead_letter_queue(&self, service: &str) -> Result<bool, ExecutionClientError> {
        #[derive(serde::Deserialize)]
        struct ReplayResponse {
            replayed: bool,
        }
        let Some((_, url)) = self.dead_letter_services.iter().find(|(name, _)| name == service)
        else {
            return Err(ExecutionClientError::Rejected(404));
        };
        let response = self
            .client
            .post(format!("{url}/v1/dead-letter/replay"))
            .send()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(ExecutionClientError::Rejected(response.status().as_u16()));
        }
        response
            .json()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))
            .map(|body: ReplayResponse| body.replayed)
    }

    async fn analysis_resilience(
        &self,
    ) -> Result<Option<AnalysisResilienceSummary>, ExecutionClientError> {
        #[derive(serde::Deserialize)]
        struct ResilienceResponse {
            consumer_alive: bool,
            fallback_configured: bool,
            dead_letter_count: u32,
        }
        let Some((_, url)) =
            self.dead_letter_services.iter().find(|(name, _)| name == "analysis-service")
        else {
            return Ok(None);
        };
        let response = self
            .client
            .get(format!("{url}/v1/resilience"))
            .send()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(ExecutionClientError::Rejected(response.status().as_u16()));
        }
        let body = response
            .json::<ResilienceResponse>()
            .await
            .map_err(|e| ExecutionClientError::Unreachable(e.to_string()))?;
        Ok(Some(AnalysisResilienceSummary {
            consumer_alive: body.consumer_alive,
            fallback_configured: body.fallback_configured,
            dead_letter_count: body.dead_letter_count,
        }))
    }
}
