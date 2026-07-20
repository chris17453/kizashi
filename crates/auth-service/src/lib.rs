//! Auth Service (spec §6, service #10): local login (Argon2id-hashed credentials) and unified
//! Entra/generic-OAuth OIDC login (ADR-0009), both minting sessions via Query Gateway's
//! internal API rather than writing into its token table directly (spec §2 principle 1).

#[path = "lib_test.rs"]
#[cfg(test)]
mod lib_test;

mod audit_log;
mod branding_handler;
mod health;
mod internal_secret;
mod local_login_handler;
mod local_user_repository;
mod oidc_client;
mod oidc_handler;
mod password;
mod session_client;
mod tenant_branding_repository;
mod tenant_repository;
mod user_handlers;

pub use audit_log::{
    AuditLogEntry, AuditLogError, AuditLogReader, ChangeType, PostgresAuditLogReader,
};
pub use branding_handler::{get_branding, get_branding_by_id, put_branding};
pub use health::build_router as health_router;
pub use internal_secret::require_internal_secret;
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
pub use tenant_branding_repository::{
    PostgresTenantBrandingRepository, TenantBranding, TenantBrandingRepository,
    TenantBrandingRepositoryError,
};
pub use tenant_repository::{PostgresTenantRepository, TenantRepository, TenantRepositoryError};
pub use user_handlers::{
    create_user, delete_user, get_recent_audit_log, get_user_audit_log, list_users,
    update_user_role, CreateUserRequest, RecentAuditLogQuery, UpdateUserRoleRequest,
};

use axum::routing::{get, post, put};
use axum::Router;

/// `internal_secret` is the same `INTERNAL_API_SECRET` value already read once in `main.rs` and
/// threaded into `HttpSessionClient` (ADR-0009) — reused here rather than reading the env var a
/// second time, gating only the routes that trust `X-Role`/`X-Tenant-Id`/`X-Username`.
pub fn build_router(state: AuthState, internal_secret: String) -> Router {
    // Pre-session, browser-facing entry points: a real end user's browser (via the Console UI's
    // backend-to-backend call) hits these directly before any session/role exists, so none of
    // them read the trust headers the gate below protects — gating them would break login.
    let public_routes = Router::new()
        .route("/v1/auth/local/login", post(local_login))
        .route("/v1/auth/oidc/:provider/authorize", get(authorize))
        .route("/v1/auth/oidc/:provider/callback", post(callback))
        // `GET` is the Console UI's login page (unauthenticated, workspace-name-keyed);
        // deliberately unauthenticated per `branding_handler::get_branding`'s doc comment.
        .route("/v1/tenants/:name/branding", get(get_branding))
        .route("/v1/tenants/id/:id/branding", get(get_branding_by_id))
        .with_state(state.clone());

    // Everything below is reached only via an already-authenticated Console UI session and
    // trusts `X-Role`/`X-Tenant-Id`/`X-Username` at face value — gated so only the Console UI's
    // own backend (which knows `INTERNAL_API_SECRET`) can reach it.
    let protected_routes = Router::new()
        // `PUT` here is the admin-only branding save; same `:name` path segment as the public
        // `GET` above, merged into one route by axum, but only this half carries the gate.
        .route("/v1/tenants/:name/branding", put(put_branding))
        .route("/v1/users", post(create_user).get(list_users))
        .route("/v1/users/:id", put(update_user_role).delete(delete_user))
        // Same shape as config-admin-service's/retention-service's `GET
        // /v1/audit-log/:entity_id` (Console UI's `AuditLogClient` is written once against
        // that shared shape and reused for every backend that owns an audited entity type).
        .route("/v1/audit-log/:entity_id", get(get_user_audit_log))
        // General, chronological "recent activity" trail across all entities (no entity_id
        // segment — axum disambiguates this exact path from the `:entity_id` one above by
        // shape). Same protected group so it inherits the `X-Internal-Secret` gate below.
        .route("/v1/audit-log", get(get_recent_audit_log))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(internal_secret, require_internal_secret));

    public_routes.merge(protected_routes)
}
