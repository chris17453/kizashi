#[path = "cookie_security_test.rs"]
#[cfg(test)]
mod cookie_security_test;

/// Pure logic split out from `cookie_secure()` so it's testable without mutating process-global
/// env state (unsafe across parallel test threads).
fn secure_from_env_value(value: Option<&str>) -> bool {
    value == Some("true")
}

/// Whether Console UI is deployed behind TLS, so its `Set-Cookie` headers should carry the
/// `Secure` attribute (browsers then refuse to send the cookie over plain HTTP — standard
/// OWASP session-management hardening). Defaults to `false` so local/dev over plain HTTP keeps
/// working without extra setup; real deployments set `COOKIE_SECURE=true`.
pub fn cookie_secure() -> bool {
    secure_from_env_value(std::env::var("COOKIE_SECURE").ok().as_deref())
}

/// The `; Secure` suffix to append to a `Set-Cookie` header value, or nothing.
pub fn cookie_secure_suffix(secure: bool) -> &'static str {
    if secure {
        "; Secure"
    } else {
        ""
    }
}
