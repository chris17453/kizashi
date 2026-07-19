//! Ingestion Gateway (spec §6, service #2): the single agent-facing entry point. Authenticates
//! connectors/agents via API key, applies per-tenant rate limiting, and routes to Ingestion
//! Service with the authenticated tenant_id, never a client-supplied one.

mod agent_status_client;
mod api_key_store;
mod health;
mod ingest_proxy_handler;
mod rate_limiter;

pub use agent_status_client::{
    AgentStatus, AgentStatusClient, AgentStatusError, HttpAgentStatusClient,
};
pub use api_key_store::{hash_api_key, ApiKeyStore, ApiKeyStoreError, PostgresApiKeyStore};
pub use ingest_proxy_handler::{ingest_proxy, GatewayState};
pub use rate_limiter::{Clock, RateLimiter, SystemClock};

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/v1/ingest", post(ingest_proxy))
        .with_state(state)
}
