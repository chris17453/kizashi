use super::*;

#[test]
fn new_ids_are_unique() {
    assert_ne!(SignalId::new(), SignalId::new());
}

#[test]
fn display_is_26_char_ulid_and_round_trips_through_from_str() {
    let id = SignalId::new();
    let text = id.to_string();
    assert_eq!(text.len(), 26);
    assert_eq!(text.parse::<SignalId>().unwrap(), id);
}

#[test]
fn from_str_rejects_garbage_without_panicking() {
    let err = "not-a-ulid".parse::<SignalId>().unwrap_err();
    assert!(err.to_string().contains("not-a-ulid"));
    assert!("".parse::<SignalId>().is_err());
}

#[test]
fn serializes_as_plain_json_string() {
    let ulid = Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
    let id = SignalId::from_ulid(ulid);
    assert_eq!(id.as_ulid(), ulid);
    assert_eq!(serde_json::to_string(&id).unwrap(), "\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"");
    let back: SignalId = serde_json::from_str("\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"").unwrap();
    assert_eq!(back, id);
}

#[test]
fn default_produces_a_fresh_id() {
    assert_ne!(SignalId::default(), SignalId::default());
}

#[test]
fn later_ids_sort_after_earlier_ones_across_milliseconds() {
    let first = SignalId::new();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = SignalId::new();
    assert!(first < second);
}
