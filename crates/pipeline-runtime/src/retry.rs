#[cfg(test)]
#[path = "retry_test.rs"]
mod retry_test;

/// A bounded retry budget prevents an unavailable downstream system from becoming an infinite
/// work loop. Exhausted messages remain durable for operator dead-letter recovery.
pub const MAX_OUTBOX_PUBLISH_ATTEMPTS: i32 = 8;

pub fn should_dead_letter(attempts: i32) -> bool {
    attempts >= MAX_OUTBOX_PUBLISH_ATTEMPTS
}
