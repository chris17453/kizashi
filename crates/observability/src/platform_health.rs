#[path = "platform_health_test.rs"]
#[cfg(test)]
pub(crate) mod platform_health_test;

use crate::service_registry::ServiceEndpoint;
use async_trait::async_trait;
use futures_util::future::join_all;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceHealth {
    pub name: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlatformHealth {
    pub status: Status,
    pub services: Vec<ServiceHealth>,
}

/// Checks a single service's `/healthz` — abstracted so the fan-out logic below is testable
/// without real HTTP calls (CLAUDE.md §2).
#[async_trait]
pub trait ServiceHealthChecker: Send + Sync {
    async fn check(&self, endpoint: &ServiceEndpoint) -> Status;
}

pub struct HttpServiceHealthChecker {
    client: reqwest::Client,
}

impl HttpServiceHealthChecker {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ServiceHealthChecker for HttpServiceHealthChecker {
    async fn check(&self, endpoint: &ServiceEndpoint) -> Status {
        match self.client.get(format!("{}/healthz", endpoint.url)).send().await {
            Ok(response) if response.status().is_success() => Status::Up,
            _ => Status::Down,
        }
    }
}

/// Fans `checker.check` out concurrently across every registered service (ADR-0012) — overall
/// platform status is `Down` if any one service is down, since "is everything up" is the
/// question this answers.
pub async fn check_platform_health(
    checker: &dyn ServiceHealthChecker,
    registry: &[ServiceEndpoint],
) -> PlatformHealth {
    let checks = registry.iter().map(|endpoint| async move {
        let status = checker.check(endpoint).await;
        ServiceHealth { name: endpoint.name.clone(), status }
    });
    let services: Vec<ServiceHealth> = join_all(checks).await;

    let status =
        if services.iter().all(|s| s.status == Status::Up) { Status::Up } else { Status::Down };

    PlatformHealth { status, services }
}
