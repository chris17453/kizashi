#[path = "proxy_auth_test.rs"]
#[cfg(test)]
mod proxy_auth_test;

use base64::Engine;

/// The caller identity attributed to one CONNECT request (ADR-0021) — carried via
/// `Proxy-Authorization: Basic base64(tenant_id:connector_id)`, the same HTTP mechanism every
/// proxy-aware client (including `reqwest::Proxy::basic_auth`) already implements, so no new
/// client-side protocol work is needed to adopt this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerIdentity {
    pub tenant_id: String,
    pub connector_id: String,
}

/// Parses a `Proxy-Authorization` header value. Returns `None` for anything malformed —
/// callers treat that as "unattributed," logged but not rejected in v1 (ADR-0021
/// Consequences), never a panic on attacker-controlled input.
pub fn parse_proxy_authorization(header_value: &str) -> Option<CallerIdentity> {
    let encoded = header_value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (tenant_id, connector_id) = decoded.split_once(':')?;
    if tenant_id.is_empty() || connector_id.is_empty() {
        return None;
    }
    Some(CallerIdentity {
        tenant_id: tenant_id.to_string(),
        connector_id: connector_id.to_string(),
    })
}
