#[path = "target_test.rs"]
#[cfg(test)]
mod target_test;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectTarget {
    pub host: String,
    pub port: u16,
}

/// Parses a CONNECT request's authority-form target (`host:port`, per RFC 7231 §4.3.6 — the
/// only form a CONNECT request line ever uses). Never panics on malformed input; a client
/// sending a bad target just gets a rejected proxy attempt, not a crashed gateway.
pub fn parse_connect_target(authority: &str) -> Option<ConnectTarget> {
    let (host, port) = authority.rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    Some(ConnectTarget { host: host.to_string(), port })
}
