use super::*;
use lapin::types::{AMQPValue, FieldTable};

#[test]
fn retry_count_is_zero_when_headers_are_absent() {
    assert_eq!(retry_count(None), 0);
}

#[test]
fn retry_count_is_zero_when_the_header_is_not_present() {
    let headers = FieldTable::default();
    assert_eq!(retry_count(Some(&headers)), 0);
}

#[test]
fn retry_count_reads_the_stored_value() {
    let mut headers = FieldTable::default();
    headers.insert(RETRY_COUNT_HEADER.into(), AMQPValue::LongUInt(3));
    assert_eq!(retry_count(Some(&headers)), 3);
}

#[test]
fn with_incremented_retry_count_sets_the_header_to_one_more_than_before() {
    let headers = with_incremented_retry_count(None);
    assert_eq!(retry_count(Some(&headers)), 1);

    let headers = with_incremented_retry_count(Some(&headers));
    assert_eq!(retry_count(Some(&headers)), 2);
}

#[test]
fn should_dead_letter_is_false_below_the_max_and_true_at_or_above_it() {
    let mut headers = FieldTable::default();
    headers.insert(RETRY_COUNT_HEADER.into(), AMQPValue::LongUInt(MAX_RETRIES - 1));
    assert!(!should_dead_letter(Some(&headers)));

    let mut headers = FieldTable::default();
    headers.insert(RETRY_COUNT_HEADER.into(), AMQPValue::LongUInt(MAX_RETRIES));
    assert!(should_dead_letter(Some(&headers)));
}
