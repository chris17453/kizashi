#[path = "health_test.rs"]
#[cfg(test)]
mod health_test;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Router};

/// How long the main consume loop can go without ticking before we consider it stuck. Must
/// exceed both the loop's own idle-timeout (`max_wait`, default 500ms) and the worst-case time
/// a real (non-stuck) batch can spend inside a single AI backend call by a wide margin, so
/// neither normal idle periods nor legitimately slow-but-bounded AI calls trip it. The AI HTTP
/// client has a 30s per-request timeout and a batch can require multiple sequential rounds of
/// up to `openai_compatible_concurrency` (default 4) concurrent calls — with the default batch
/// size of 20 that's up to 5 rounds, ~150s worst case — so this must be comfortably above that.
pub const STALE_THRESHOLD: Duration = Duration::from_secs(180);

/// Tracks liveness of the record.normalized consume loop. The loop calls `tick()` on every
/// select iteration (including the deadline-timeout branch, which fires even when the queue is
/// empty) and once per tenant group before processing its batch, so a stale heartbeat means the
/// loop has stopped scheduling entirely, not just that a single AI call is taking a while.
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
