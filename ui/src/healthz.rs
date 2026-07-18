#[path = "healthz_test.rs"]
#[cfg(test)]
mod healthz_test;

/// GET /healthz — plain liveness check, same convention as every other service in this repo.
/// Distinct from `/health`, the authenticated platform-health dashboard page
/// (`health_handler.rs`) — this one needs no session and answers "is this process up," not
/// "is the platform up."
pub async fn healthz() -> &'static str {
    "ok"
}
