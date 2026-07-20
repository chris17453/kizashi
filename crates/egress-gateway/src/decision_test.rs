use super::*;
use crate::allowlist::allowlist_test::InMemoryAllowlistRepository;
use crate::audit_log::audit_log_test::InMemoryAuditLogRepository;
use crate::proxy_auth::CallerIdentity;
use crate::target::ConnectTarget;
use std::sync::Arc;

fn deps_with_audit_log() -> (ProxyDeps, Arc<InMemoryAuditLogRepository>) {
    let audit_log_repository = Arc::new(InMemoryAuditLogRepository::default());
    let deps = ProxyDeps {
        allowlist_repository: Arc::new(InMemoryAllowlistRepository::default()),
        audit_log_repository: audit_log_repository.clone(),
    };
    (deps, audit_log_repository)
}

#[tokio::test]
async fn allows_and_logs_when_no_allowlist_is_configured() {
    let (deps, _audit_log) = deps_with_audit_log();
    let identity = Some(CallerIdentity {
        tenant_id: "tenant-a".to_string(),
        connector_id: "zendesk-connector".to_string(),
    });
    let target = ConnectTarget { host: "api.zendesk.com".to_string(), port: 443 };

    let decision = decide(&deps, identity.as_ref(), &target).await;

    assert!(decision.allowed);
}

#[tokio::test]
async fn denies_when_the_tenant_has_an_allowlist_that_excludes_the_host() {
    let (deps, _audit_log) = deps_with_audit_log();
    deps.allowlist_repository
        .set_domains("tenant-a", vec!["zendesk.com".to_string()], "test-actor")
        .await
        .unwrap();
    let identity = Some(CallerIdentity {
        tenant_id: "tenant-a".to_string(),
        connector_id: "zendesk-connector".to_string(),
    });
    let target = ConnectTarget { host: "evil.example.com".to_string(), port: 443 };

    let decision = decide(&deps, identity.as_ref(), &target).await;

    assert!(!decision.allowed);
}

#[tokio::test]
async fn allows_when_the_host_matches_the_tenants_allowlist() {
    let (deps, _audit_log) = deps_with_audit_log();
    deps.allowlist_repository
        .set_domains("tenant-a", vec!["zendesk.com".to_string()], "test-actor")
        .await
        .unwrap();
    let identity = Some(CallerIdentity {
        tenant_id: "tenant-a".to_string(),
        connector_id: "zendesk-connector".to_string(),
    });
    let target = ConnectTarget { host: "acme.zendesk.com".to_string(), port: 443 };

    let decision = decide(&deps, identity.as_ref(), &target).await;

    assert!(decision.allowed);
}

#[tokio::test]
async fn unattributed_requests_are_allowed_but_logged_as_unknown() {
    let (deps, audit_log) = deps_with_audit_log();
    let target = ConnectTarget { host: "api.zendesk.com".to_string(), port: 443 };

    let decision = decide(&deps, None, &target).await;

    assert!(decision.allowed);
    let entries = audit_log.entries.lock().unwrap();
    assert_eq!(entries[0].tenant_id, "unknown");
    assert_eq!(entries[0].connector_id, "unknown");
}

#[tokio::test]
async fn every_decision_writes_exactly_one_audit_entry() {
    let (deps, audit_log) = deps_with_audit_log();
    let target = ConnectTarget { host: "api.zendesk.com".to_string(), port: 443 };

    decide(&deps, None, &target).await;

    assert_eq!(audit_log.entries.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_denied_decision_is_logged_as_not_allowed() {
    let (deps, audit_log) = deps_with_audit_log();
    deps.allowlist_repository
        .set_domains("tenant-a", vec!["zendesk.com".to_string()], "test-actor")
        .await
        .unwrap();
    let identity = Some(CallerIdentity {
        tenant_id: "tenant-a".to_string(),
        connector_id: "zendesk-connector".to_string(),
    });
    let target = ConnectTarget { host: "evil.example.com".to_string(), port: 443 };

    decide(&deps, identity.as_ref(), &target).await;

    let entries = audit_log.entries.lock().unwrap();
    assert!(!entries[0].allowed);
}
