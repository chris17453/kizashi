#[path = "decision_test.rs"]
#[cfg(test)]
mod decision_test;

use crate::allowlist::{is_host_allowed, AllowlistRepository};
use crate::audit_log::{AuditLogEntry, AuditLogRepository};
use crate::proxy_auth::CallerIdentity;
use crate::target::ConnectTarget;
use std::sync::Arc;

#[derive(Clone)]
pub struct ProxyDeps {
    pub allowlist_repository: Arc<dyn AllowlistRepository>,
    pub audit_log_repository: Arc<dyn AuditLogRepository>,
}

pub struct Decision {
    pub allowed: bool,
}

/// The full policy + audit decision for one CONNECT request (ADR-0021): looks up the calling
/// tenant's allowlist (if any), decides allow/deny, and writes exactly one audit row either
/// way — logging happens before the caller ever finds out the outcome, so a crash/disconnect
/// after this call still leaves an accurate audit trail.
pub async fn decide(
    deps: &ProxyDeps,
    identity: Option<&CallerIdentity>,
    target: &ConnectTarget,
) -> Decision {
    let (tenant_id, connector_id) = match identity {
        Some(id) => (id.tenant_id.as_str(), id.connector_id.as_str()),
        None => ("unknown", "unknown"),
    };

    let allowlist = deps.allowlist_repository.get_domains(tenant_id).await.unwrap_or_else(|e| {
        tracing::error!(tenant_id, error = %e, "failed to read allowlist, defaulting to allow");
        Vec::new()
    });
    let allowed = is_host_allowed(&allowlist, &target.host);

    let entry = AuditLogEntry {
        tenant_id: tenant_id.to_string(),
        connector_id: connector_id.to_string(),
        destination_host: target.host.clone(),
        destination_port: target.port,
        allowed,
        occurred_at: chrono::Utc::now(),
    };
    if let Err(e) = deps.audit_log_repository.record(entry).await {
        tracing::error!(tenant_id, error = %e, "failed to write egress audit log entry");
    }

    Decision { allowed }
}
