//! Query Gateway (spec §6, service #8): the single dashboard/UI-facing entry point.
//! Authenticates callers via bearer token (ADR-0008), resolves a tenant, and forwards read
//! requests to Dashboard/Query API with `X-Tenant-Id` set from the *authenticated* identity.

mod health;
mod internal_handler;
mod proxy_handler;
mod token_store;

pub use health::build_router as health_router;
pub use internal_handler::{mint_token, MintTokenRequest, MintTokenResponse};
pub use proxy_handler::{proxy_any, proxy_get, GatewayState};
pub use token_store::{hash_token, PostgresTokenStore, TokenStore, TokenStoreError};

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/events", get(proxy_get))
        .route("/v1/events/daily-counts", get(proxy_get))
        .route("/v1/events/:id", axum::routing::get(proxy_get).patch(proxy_any))
        .route("/v1/events/:id/status-history", get(proxy_get))
        .route("/api/ontology/objects/types", axum::routing::any(proxy_any))
        .route("/api/ontology/objects/types/:id", axum::routing::any(proxy_any))
        .route("/api/ontology/links/types", axum::routing::any(proxy_any))
        .route("/api/ontology/links/types/:id", axum::routing::any(proxy_any))
        .route("/api/ontology/links", axum::routing::any(proxy_any))
        .route("/api/ontology/links/:id", axum::routing::any(proxy_any))
        .route("/api/ontology/objects", axum::routing::any(proxy_any))
        .route("/api/ontology/objects/:id", axum::routing::any(proxy_any))
        .route("/api/ontology/objects/:id/links/:link_type_id", axum::routing::any(proxy_any))
        .route("/api/ontology/actions/invocations", axum::routing::any(proxy_any))
        .route("/api/ontology/actions/reviews", axum::routing::any(proxy_any))
        .route("/api/ontology/actions/invoke", axum::routing::any(proxy_any))
        .route("/api/ontology/actions/types", axum::routing::any(proxy_any))
        .route("/api/ontology/actions/types/:id", axum::routing::any(proxy_any))
        .route("/api/ontology/actions/types/:id/history", axum::routing::any(proxy_any))
        .route("/internal/tokens", post(mint_token))
        .with_state(state)
}
