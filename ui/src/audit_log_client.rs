#[path = "audit_log_client_test.rs"]
#[cfg(test)]
pub(crate) mod audit_log_client_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub change_type: String,
    pub actor: String,
    pub before: Option<serde_json::Value>,
    pub after: serde_json::Value,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum AuditLogClientError {
    #[error("service unreachable: {0}")]
    Unreachable(String),
    #[error("service rejected the request: HTTP {0}")]
    Rejected(u16),
}

/// Reads the immutable audit trail CLAUDE.md §5 requires for every admin/config mutation, via
/// whichever backend owns the entity — `config-admin-service` (triggers, mappings, agents,
/// analysis config) and `retention-service` (retention policies) both expose an identically
/// shaped `GET /v1/audit-log/:entity_id`, so one client implementation, constructed once per
/// backend base URL, covers both (see `AppState::config_audit_log_client`/
/// `retention_audit_log_client`). This closes the last "backend exists, UI can't see it" gap
/// found in the Console UI completeness audit — every write page already writes to this trail,
/// but nothing could read it back before now.
#[async_trait]
pub trait AuditLogClient: Send + Sync {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError>;

    /// Most-recent-first activity feed across every entity for the tenant (ADR-0045) — powers
    /// the global `/audit-log` page, distinct from `list_for_entity`'s single-entity history.
    /// `before` is a cursor (the `changed_at` of the oldest entry already shown) for "load
    /// older" pagination; `None` starts from the most recent entry.
    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: u32,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError>;
}

pub struct HttpAuditLogClient {
    client: reqwest::Client,
    base_url: String,
}

impl HttpAuditLogClient {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

/// Reads `ingestion-gateway`'s per-API-key audit trail. Unlike `HttpAuditLogClient`'s three
/// backends, `ingestion-gateway` exposes this at `GET /v1/api-keys/:id/audit-log` (an
/// entity-scoped route, not the shared `GET /v1/audit-log/:entity_id` shape) and has no
/// tenant-wide "recent activity" feed of its own -- API key changes are only ever viewed via a
/// specific key's own "History" link, never folded into the global `/audit-log` page's
/// multi-service feed. `list_recent` is unsupported: any caller reaching it is a bug (the
/// `/audit-log` handler's `service` switch never routes there), and an empty `Ok(vec![])` would
/// hide that bug rather than surface it.
pub struct IngestionGatewayApiKeyAuditLogClient {
    client: reqwest::Client,
    base_url: String,
}

impl IngestionGatewayApiKeyAuditLogClient {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

#[async_trait]
impl AuditLogClient for IngestionGatewayApiKeyAuditLogClient {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        let response = self
            .client
            .get(format!("{}/v1/api-keys/{entity_id}/audit-log", self.base_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| AuditLogClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AuditLogClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AuditLogClientError::Unreachable(e.to_string()))
    }

    async fn list_recent(
        &self,
        _tenant_id: Uuid,
        _limit: u32,
        _before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        // Never actually reached in production (nothing routes the global /audit-log page to
        // this client) -- an error, not a panic, so a future routing mistake degrades to a
        // rendered error message instead of taking the whole request handler down.
        Err(AuditLogClientError::Unreachable(
            "ingestion-gateway has no tenant-wide audit feed".to_string(),
        ))
    }
}

#[async_trait]
impl AuditLogClient for HttpAuditLogClient {
    async fn list_for_entity(
        &self,
        tenant_id: Uuid,
        entity_id: Uuid,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        let response = self
            .client
            .get(format!("{}/v1/audit-log/{entity_id}", self.base_url))
            .header("x-tenant-id", tenant_id.to_string())
            .send()
            .await
            .map_err(|e| AuditLogClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AuditLogClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AuditLogClientError::Unreachable(e.to_string()))
    }

    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: u32,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditLogEntry>, AuditLogClientError> {
        let mut request = self
            .client
            .get(format!("{}/v1/audit-log", self.base_url))
            .header("x-tenant-id", tenant_id.to_string())
            .query(&[("limit", limit.to_string())]);
        if let Some(before) = before {
            request = request.query(&[("before", before.to_rfc3339())]);
        }
        let response =
            request.send().await.map_err(|e| AuditLogClientError::Unreachable(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AuditLogClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| AuditLogClientError::Unreachable(e.to_string()))
    }
}
