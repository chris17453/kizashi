#[path = "backup_status_client_test.rs"]
#[cfg(test)]
pub(crate) mod backup_status_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct BackupRun {
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: String,
    pub target: String,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Error)]
pub enum BackupStatusClientError {
    #[error("backup service unreachable: {0}")]
    Unreachable(String),
    #[error("backup service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Console UI's client for Backup Service's run history (ADR-0055) --
/// `GET /v1/backup/status`, Admin-only on the backend, same direct-call trust boundary as
/// every other write-path/ops client (ADR-0010).
#[async_trait]
pub trait BackupStatusClient: Send + Sync {
    async fn list_recent(
        &self,
        role: common::Role,
    ) -> Result<Vec<BackupRun>, BackupStatusClientError>;
}

pub struct HttpBackupStatusClient {
    client: reqwest::Client,
    backup_service_url: String,
}

impl HttpBackupStatusClient {
    pub fn new(client: reqwest::Client, backup_service_url: String) -> Self {
        Self { client, backup_service_url }
    }
}

#[async_trait]
impl BackupStatusClient for HttpBackupStatusClient {
    async fn list_recent(
        &self,
        role: common::Role,
    ) -> Result<Vec<BackupRun>, BackupStatusClientError> {
        let response = self
            .client
            .get(format!("{}/v1/backup/status", self.backup_service_url))
            .header("x-role", role.to_string())
            .send()
            .await
            .map_err(|e| BackupStatusClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BackupStatusClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| BackupStatusClientError::Unreachable(e.to_string()))
    }
}
