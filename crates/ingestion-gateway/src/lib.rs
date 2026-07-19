//! Ingestion Gateway (spec §6, service #2): the single agent-facing entry point. Authenticates
//! connectors/agents via API key, applies per-tenant rate limiting, and routes to Ingestion
//! Service with the authenticated tenant_id, never a client-supplied one.

mod agent_status_client;
mod api_key_handlers;
mod api_key_store;
mod audit_log;
mod health;
mod ingest_proxy_handler;
mod rate_limiter;

pub use agent_status_client::{
    AgentStatus, AgentStatusClient, AgentStatusError, HttpAgentStatusClient,
};
pub use api_key_handlers::{create_api_key, get_api_key_audit_log, list_api_keys, revoke_api_key};
pub use api_key_store::{
    hash_api_key, ApiKeyStore, ApiKeyStoreError, ApiKeySummary, PostgresApiKeyStore,
};
pub use audit_log::{
    AuditLogEntry, AuditLogError, AuditLogReader, ChangeType, PostgresAuditLogReader,
};
pub use ingest_proxy_handler::{ingest_proxy, GatewayState};
pub use rate_limiter::{Clock, RateLimiter, SystemClock};

use axum::routing::{delete, get, post};
use axum::Router;

pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/ingest", post(ingest_proxy))
        .route("/v1/api-keys", get(list_api_keys).post(create_api_key))
        .route("/v1/api-keys/:id", delete(revoke_api_key))
        .route("/v1/api-keys/:id/audit-log", get(get_api_key_audit_log))
        .with_state(state)
}
