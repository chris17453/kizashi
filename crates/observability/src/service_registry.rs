#[path = "service_registry_test.rs"]
#[cfg(test)]
mod service_registry_test;

/// One entry in the platform's health-check fan-out list (ADR-0012) — a service name paired
/// with its base URL (`GET {url}/healthz` is what gets called).
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceEndpoint {
    pub name: String,
    pub url: String,
}

/// Parses `SERVICE_REGISTRY`'s `name=url,name2=url2` format. Operator-configured (ADR-0012 —
/// no automatic service discovery), so this is deliberately forgiving: a malformed entry is
/// skipped with a logged warning rather than failing the whole registry, since one typo
/// shouldn't blind the platform to every other service's health.
pub fn parse_registry(raw: &str) -> Vec<ServiceEndpoint> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| match entry.split_once('=') {
            Some((name, url)) if !name.trim().is_empty() && !url.trim().is_empty() => {
                Some(ServiceEndpoint { name: name.trim().to_string(), url: url.trim().to_string() })
            }
            _ => {
                tracing::warn!(entry, "skipping malformed SERVICE_REGISTRY entry");
                None
            }
        })
        .collect()
}
