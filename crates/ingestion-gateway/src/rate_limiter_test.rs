use super::*;
use std::sync::Arc;

/// Test clock: starts at `Instant::now()` at construction and advances only when told to,
/// so window-boundary behavior can be tested deterministically. Shared via `Arc` so the test
/// can advance it after handing a clone to the `RateLimiter`.
pub struct TestClock {
    base: Instant,
    offset: Mutex<Duration>,
}

impl TestClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { base: Instant::now(), offset: Mutex::new(Duration::ZERO) })
    }

    pub fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }
}

impl Clock for Arc<TestClock> {
    fn now(&self) -> Instant {
        self.base + *self.offset.lock().unwrap()
    }
}

#[test]
fn allows_requests_up_to_the_limit_within_a_window() {
    let limiter = RateLimiter::new(3, Duration::from_secs(60), Box::new(TestClock::new()));
    let tenant_id = Uuid::new_v4();

    assert!(limiter.check(tenant_id));
    assert!(limiter.check(tenant_id));
    assert!(limiter.check(tenant_id));
    assert!(!limiter.check(tenant_id), "fourth request within the window must be rejected");
}

#[test]
fn resets_after_the_window_elapses() {
    let clock = TestClock::new();
    let limiter = RateLimiter::new(1, Duration::from_secs(60), Box::new(clock.clone()));
    let tenant_id = Uuid::new_v4();

    assert!(limiter.check(tenant_id));
    assert!(!limiter.check(tenant_id));

    clock.advance(Duration::from_secs(61));
    assert!(limiter.check(tenant_id), "request after window elapses must be allowed again");
}

#[test]
fn tenants_have_independent_budgets() {
    let limiter = RateLimiter::new(1, Duration::from_secs(60), Box::new(TestClock::new()));
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    assert!(limiter.check(tenant_a));
    assert!(!limiter.check(tenant_a));
    assert!(limiter.check(tenant_b), "a different tenant's budget must be unaffected");
}
