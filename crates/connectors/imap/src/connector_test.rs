use super::*;

fn sample_connector() -> ImapConnector {
    ImapConnector::new(
        "imap-inbox",
        "imap.example.com",
        993,
        "user@example.com",
        "secret",
        "INBOX",
        chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        true,
    )
}

#[test]
fn reports_its_own_connector_id_and_source_type() {
    let c = sample_connector();
    assert_eq!(c.connector_id(), "imap-inbox");
    assert_eq!(c.source_type(), SourceType::Message);
}

fn record_with_uid(uid: u32) -> common::RawRecord {
    common::RawRecord::new(
        "imap-inbox",
        SourceType::Message,
        uuid::Uuid::new_v4(),
        serde_json::json!({"uid": uid}),
    )
}

#[test]
fn checkpoint_is_the_highest_uid_seen_across_the_polled_records() {
    let c = sample_connector();
    let records = vec![record_with_uid(5), record_with_uid(42), record_with_uid(17)];
    assert_eq!(c.checkpoint(&records), Some("42".to_string()));
}

#[test]
fn checkpoint_is_none_for_an_empty_poll() {
    let c = sample_connector();
    assert_eq!(c.checkpoint(&[]), None);
}

#[test]
fn search_query_uses_uid_range_when_since_uid_is_set() {
    let c = sample_connector().with_since_uid(Some(100));
    assert_eq!(c.search_query(), "UID 101:*");
}

#[test]
fn search_query_falls_back_to_since_date_when_since_uid_is_absent() {
    let c = sample_connector();
    assert_eq!(c.search_query(), "SINCE 01-Jan-2024");
}

#[test]
fn select_uids_sorts_ascending_and_is_unbounded_by_default() {
    assert_eq!(select_uids(vec![30, 10, 20], None), vec![10, 20, 30]);
}

#[test]
fn select_uids_caps_to_the_oldest_n_when_a_max_is_set() {
    // Oldest-first, not newest-first: this is what turns a large backfill into ordered
    // chunks — each poll picks up exactly where the last one's checkpoint left off.
    assert_eq!(select_uids(vec![30, 10, 20, 40], Some(2)), vec![10, 20]);
}

#[test]
fn select_uids_max_larger_than_available_returns_everything() {
    assert_eq!(select_uids(vec![5, 1, 3], Some(100)), vec![1, 3, 5]);
}
