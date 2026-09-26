use super::*;
use chrono::TimeZone;

pub(crate) fn sample_signal() -> Signal {
    Signal {
        signal_id: SignalId::new(),
        tenant_id: Uuid::nil(),
        entity: EntityRef::new("email", "buyer@example.com"),
        signal_type: "email.reply_latency_hours".into(),
        ts: Utc.with_ymd_and_hms(2026, 9, 26, 12, 0, 0).unwrap(),
        value: SignalValue::Gauge(3.5),
        dims: BTreeMap::from([("direction".to_string(), "inbound".to_string())]),
        extractor_version: "email-latency@1".into(),
        config_version: "cfg-1".into(),
        provenance: Provenance {
            source_item_ids: vec!["<m1@example.com>".into(), "<m2@example.com>".into()],
            model_version: None,
            encrypted: None,
        },
        contributions: vec![],
    }
}

#[test]
fn idempotency_key_is_64_char_lowercase_hex() {
    let key = sample_signal().idempotency_key();
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
}

#[test]
fn idempotency_key_ignores_signal_id_value_config_and_model_version() {
    let a = sample_signal();
    let mut b = a.clone();
    b.signal_id = SignalId::new();
    b.value = SignalValue::Gauge(99.0);
    b.config_version = "cfg-2".into();
    b.provenance.model_version = Some("m@2".into());
    assert_eq!(a.idempotency_key(), b.idempotency_key());
}

#[test]
fn idempotency_key_ignores_source_item_order_and_duplicates() {
    let a = sample_signal();
    let mut b = a.clone();
    b.provenance.source_item_ids =
        vec!["<m2@example.com>".into(), "<m1@example.com>".into(), "<m1@example.com>".into()];
    assert_eq!(a.idempotency_key(), b.idempotency_key());
}

#[test]
fn idempotency_key_changes_with_each_identifying_field() {
    let base = sample_signal();
    let base_key = base.idempotency_key();
    let mutations: Vec<fn(&mut Signal)> = vec![
        |s| s.tenant_id = Uuid::from_u128(1),
        |s| s.entity.entity_type = "domain".into(),
        |s| s.entity.entity_id = "other@example.com".into(),
        |s| s.signal_type = "email.count".into(),
        |s| s.ts += chrono::Duration::nanoseconds(1),
        |s| {
            s.dims.insert("channel".into(), "cc".into());
        },
        |s| s.provenance.source_item_ids.push("<m3@example.com>".into()),
        |s| s.extractor_version = "email-latency@2".into(),
    ];
    for (i, mutate) in mutations.into_iter().enumerate() {
        let mut changed = base.clone();
        mutate(&mut changed);
        assert_ne!(changed.idempotency_key(), base_key, "mutation #{i} did not change the key");
    }
}

#[test]
fn idempotency_key_is_not_fooled_by_field_concatenation() {
    let mut a = sample_signal();
    a.entity = EntityRef::new("ab", "c");
    let mut b = sample_signal();
    b.entity = EntityRef::new("a", "bc");
    assert_ne!(a.idempotency_key(), b.idempotency_key());
}

#[test]
fn idempotency_key_is_stable_across_releases() {
    // Pinned: changing the derivation silently would stop duplicates collapsing against rows
    // already in ClickHouse. If this must change, it is a migration, not a refactor. The
    // expected value was cross-checked against an independent Python hashlib derivation.
    assert_eq!(
        sample_signal().idempotency_key(),
        "37b037added36bf165a82039e6e89c5773d5e47a9a753ffafe99b6cbbf7b573c"
    );
}

#[test]
fn contributions_default_to_empty_when_absent_on_the_wire() {
    let mut value = serde_json::to_value(sample_signal()).unwrap();
    value.as_object_mut().unwrap().remove("contributions");
    let signal: Signal = serde_json::from_value(value).unwrap();
    assert!(signal.contributions.is_empty());
}
