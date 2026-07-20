#[path = "pending_oidc_flow_test.rs"]
#[cfg(test)]
pub(crate) mod pending_oidc_flow_test;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

/// What Console UI must remember between the two browser hops of an OIDC login (redirect to
/// the IdP, then the IdP's redirect back) — the PKCE verifier and CSRF token Auth Service
/// handed back from `/authorize`, plus the workspace name the user typed, since Auth Service's
/// `/callback` needs all three and nothing survives the round-trip except what's kept here.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingOidcFlow {
    pub provider: String,
    pub csrf_token: String,
    pub code_verifier: String,
    pub tenant_name: String,
}

/// Deliberately separate from `SessionStore` (not reused with a marker field) — this holds
/// short-lived, single-use, pre-authentication state keyed by its own cookie
/// (`kizashi_oidc_flow`), never promoted to a real session, and must be consumed exactly once
/// (`take`) to prevent a captured callback URL from being replayed.
#[async_trait]
pub trait PendingOidcFlowStore: Send + Sync {
    async fn create(&self, flow: PendingOidcFlow) -> String;
    async fn get(&self, id: &str) -> Option<PendingOidcFlow>;
    async fn take(&self, id: &str) -> Option<PendingOidcFlow>;
}

#[derive(Default)]
pub struct InMemoryPendingOidcFlowStore {
    flows: Mutex<HashMap<String, PendingOidcFlow>>,
}

#[async_trait]
impl PendingOidcFlowStore for InMemoryPendingOidcFlowStore {
    async fn create(&self, flow: PendingOidcFlow) -> String {
        let id = Uuid::new_v4().to_string();
        self.flows.lock().unwrap().insert(id.clone(), flow);
        id
    }

    async fn get(&self, id: &str) -> Option<PendingOidcFlow> {
        self.flows.lock().unwrap().get(id).cloned()
    }

    async fn take(&self, id: &str) -> Option<PendingOidcFlow> {
        self.flows.lock().unwrap().remove(id)
    }
}
