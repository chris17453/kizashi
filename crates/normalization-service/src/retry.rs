#[path = "retry_test.rs"]
#[cfg(test)]
mod retry_test;

use lapin::types::{AMQPValue, FieldTable, LongUInt};

/// Header carrying the number of times this message has already been redelivered after a
/// processing failure. AMQP's own `redelivered` flag doesn't count attempts, so we track it
/// ourselves by republishing with an incremented header instead of relying on `nack(requeue)`.
pub const RETRY_COUNT_HEADER: &str = "x-normalization-retry-count";

/// A permanently-failing message (e.g. a malformed mapping that always errors) is dead-lettered
/// after this many attempts instead of being requeued forever, so it can't starve other
/// tenants' messages in the same single-consumer FIFO queue -- same rationale and threshold as
/// analysis-service's identical mechanism.
pub const MAX_RETRIES: LongUInt = 5;

pub fn retry_count(headers: Option<&FieldTable>) -> LongUInt {
    headers
        .and_then(|h| h.inner().get(RETRY_COUNT_HEADER))
        .and_then(|v| match v {
            AMQPValue::LongUInt(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(0)
}

pub fn with_incremented_retry_count(headers: Option<&FieldTable>) -> FieldTable {
    let mut table = headers.cloned().unwrap_or_default();
    table.insert(RETRY_COUNT_HEADER.into(), AMQPValue::LongUInt(retry_count(Some(&table)) + 1));
    table
}

pub fn should_dead_letter(headers: Option<&FieldTable>) -> bool {
    retry_count(headers) >= MAX_RETRIES
}
