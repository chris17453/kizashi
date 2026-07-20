#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Router};

/// How long the main consume loop can go without ticking before we consider it stuck.
/// Must exceed the loop's own idle-timeout (`max_wait`, default 500ms) by a wide margin so
/// normal idle periods never trip it.
pub const STALE_THRESHOLD: Duration = Duration::from_secs(30);

/// Tracks liveness of the record.normalized consume loop. The loop calls `tick()` on every
/// iteration — including the deadline-timeout branch, which fires even when the queue is
/// empty — so a stale heartbeat means the loop has stopped scheduling entirely, not just that
/// there's no work.
pub struct ConsumerHeartbeat {
    last_tick: Mutex<Instant>,
}

impl ConsumerHeartbeat {
    pub fn new() -> Self {
        Self { last_tick: Mutex::new(Instant::now()) }
    }

    pub fn tick(&self) {
        *self.last_tick.lock().unwrap() = Instant::now();
    }

    pub fn is_alive(&self) -> bool {
        self.last_tick.lock().unwrap().elapsed() < STALE_THRESHOLD
    }
}

impl Default for ConsumerHeartbeat {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_router(heartbeat: Arc<ConsumerHeartbeat>) -> Router {
    Router::new().route("/healthz", get(healthz)).with_state(heartbeat)
}

async fn healthz(State(heartbeat): State<Arc<ConsumerHeartbeat>>) -> StatusCode {
    if heartbeat.is_alive() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
