//! Auth Service (spec §6, service #10): local login (Argon2id-hashed credentials) and unified
//! Entra/generic-OAuth OIDC login (ADR-0009), both minting sessions via Query Gateway's
//! internal API rather than writing into its token table directly (spec §2 principle 1).

mod health;
mod local_login_handler;
mod local_user_repository;
mod oidc_client;
mod oidc_handler;
mod password;
mod session_client;
mod tenant_repository;

pub use health::build_router as health_router;
pub use local_login_handler::{local_login, AuthState, LocalLoginRequest, LoginResponse};
pub use local_user_repository::{
    LocalUser, LocalUserRepository, LocalUserRepositoryError, PostgresLocalUserRepository,
};
pub use oidc_client::{
    OidcClient, OidcError, OidcProviderConfig, OidcUserInfo, StandardOidcClient,
};
pub use oidc_handler::{authorize, callback, AuthorizeResponse, OidcCallbackRequest, OidcClients};
pub use password::{hash_password, verify_password, PasswordError};
pub use session_client::{HttpSessionClient, SessionClient, SessionClientError};
pub use tenant_repository::{PostgresTenantRepository, TenantRepository, TenantRepositoryError};

use axum::routing::{get, post};
use axum::Router;

pub fn build_router(state: AuthState) -> Router {
    Router::new()
        .route("/v1/auth/local/login", post(local_login))
        .route("/v1/auth/oidc/:provider/authorize", get(authorize))
        .route("/v1/auth/oidc/:provider/callback", post(callback))
        .with_state(state)
}
