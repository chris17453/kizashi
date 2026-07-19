//! Egress Gateway (ADR-0021): an HTTP CONNECT forward proxy every outbound `reqwest::Client`
//! in this codebase can route through, so external calls (connector polls, Action Executor's
//! webhook dispatch, OAuth2 token fetches) get a tenant/connector-scoped audit trail and an
//! optional per-tenant domain allowlist — without decrypting/inspecting HTTPS traffic itself.

mod allowlist;
mod audit_log;
mod decision;
mod health;
mod proxy_auth;
mod target;

pub use allowlist::{
    is_host_allowed, AllowlistError, AllowlistRepository, PostgresAllowlistRepository,
};
pub use audit_log::{AuditLogEntry, AuditLogError, AuditLogRepository, PostgresAuditLogRepository};
pub use decision::{decide, Decision, ProxyDeps};
pub use health::{build_router as admin_router, AdminState};
pub use proxy_auth::{parse_proxy_authorization, CallerIdentity};
pub use target::{parse_connect_target, ConnectTarget};
