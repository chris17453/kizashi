#[path = "rate_limiter_test.rs"]
#[cfg(test)]
pub(crate) mod rate_limiter_test;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Injectable clock so rate-limit window behavior is unit-testable without real sleeps
/// (CLAUDE.md §2 — unit tests must not depend on wall-clock timing).
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

struct Window {
    started_at: Instant,
    count: u32,
}

/// Per-tenant fixed-window rate limiter (spec §6, service #2: "rate limiting"). Each tenant
/// gets its own independent window so one noisy tenant can never exhaust another tenant's
/// ingestion budget (spec §8 tenant isolation).
pub struct RateLimiter {
    max_requests_per_window: u32,
    window: Duration,
    clock: Box<dyn Clock>,
    windows: Mutex<HashMap<Uuid, Window>>,
}

impl RateLimiter {
    pub fn new(max_requests_per_window: u32, window: Duration, clock: Box<dyn Clock>) -> Self {
        Self { max_requests_per_window, window, clock, windows: Mutex::new(HashMap::new()) }
    }

    /// Returns true if the request is allowed under `tenant_id`'s current window, consuming
    /// one unit of budget if so.
    pub fn check(&self, tenant_id: Uuid) -> bool {
        let now = self.clock.now();
        let mut windows = self.windows.lock().unwrap();
        let window =
            windows.entry(tenant_id).or_insert_with(|| Window { started_at: now, count: 0 });

        if now.duration_since(window.started_at) >= self.window {
            window.started_at = now;
            window.count = 0;
        }

        if window.count >= self.max_requests_per_window {
            return false;
        }
        window.count += 1;
        true
    }
}
