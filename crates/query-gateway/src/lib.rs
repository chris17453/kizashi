//! Query Gateway (spec §6, service #8): the single dashboard/UI-facing entry point.
//! Authenticates callers via bearer token (ADR-0008), resolves a tenant, and forwards read
//! requests to Dashboard/Query API with `X-Tenant-Id` set from the *authenticated* identity.

mod health;
mod internal_handler;
mod proxy_handler;
mod token_store;

pub use health::build_router as health_router;
pub use internal_handler::{mint_token, MintTokenRequest, MintTokenResponse};
pub use proxy_handler::{proxy_get, GatewayState};
pub use token_store::{hash_token, PostgresTokenStore, TokenStore, TokenStoreError};

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/events", get(proxy_get))
        .route("/v1/events/daily-counts", get(proxy_get))
        .route("/v1/events/:id", get(proxy_get))
        .route("/internal/tokens", post(mint_token))
        .with_state(state)
}
