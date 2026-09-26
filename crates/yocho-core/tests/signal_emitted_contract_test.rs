//! Schema/contract test for the `signal.emitted` bus message (docs/yocho.md "Pipeline",
//! ADR-0205), so Yochō producers (extractors, detectors, scorer) and consumers (signal writer,
//! detectors, scorer) cannot silently drift apart on the wire shape, per CLAUDE.md §2.

use chrono::{TimeZone, Utc};
use serde_json::json;
use std::collections::BTreeMap;
use ulid::Ulid;
use uuid::Uuid;
use yocho_core::{
    Contribution, EncryptedBlob, EntityRef, Provenance, Signal, SignalId, SignalValue,
    SIGNAL_EMITTED_EXCHANGE,
};

fn id(s: &str) -> SignalId {
    SignalId::from_ulid(Ulid::from_string(s).unwrap())
}

fn score_signal() -> Signal {
    Signal {
        signal_id: id("01J8Z3NDEKTSV4RRFFQ69G5FAV"),
        tenant_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
        entity: EntityRef::new("erp_customer", "C-1001"),
        signal_type: "health.score".into(),
        ts: Utc.with_ymd_and_hms(2026, 9, 26, 12, 30, 0).unwrap(),
        value: SignalValue::Score(0.42),
        dims: BTreeMap::from([("bu".to_string(), "emea".to_string())]),
        extractor_version: "health-scorer@3".into(),
        config_version: "cfg-7".into(),
        provenance: Provenance {
            source_item_ids: vec!["ticket:991".into()],
            model_version: Some("gbm@2026-09".into()),
            encrypted: Some(EncryptedBlob(vec![1, 2, 3])),
        },
        contributions: vec![Contribution {
            signal_id: id("01J8Z3NDEKTSV4RRFFQ69G5FAW"),
            weight: 0.5,
            value: -0.25,
        }],
    }
}

#[test]
fn exchange_name_is_pinned() {
    assert_eq!(SIGNAL_EMITTED_EXCHANGE, "signal.emitted");
}

#[test]
fn signal_emitted_json_shape_is_pinned() {
    let expected = json!({
        "signal_id": "01J8Z3NDEKTSV4RRFFQ69G5FAV",
        "tenant_id": "11111111-2222-3333-4444-555555555555",
        "entity": {"entity_type": "erp_customer", "entity_id": "C-1001"},
        "signal_type": "health.score",
        "ts": "2026-09-26T12:30:00Z",
        "value": {"kind": "score", "value": 0.42},
        "dims": {"bu": "emea"},
        "extractor_version": "health-scorer@3",
        "config_version": "cfg-7",
        "provenance": {
            "source_item_ids": ["ticket:991"],
            "model_version": "gbm@2026-09",
            "encrypted": "AQID"
        },
        "contributions": [
            {"signal_id": "01J8Z3NDEKTSV4RRFFQ69G5FAW", "weight": 0.5, "value": -0.25}
        ]
    });
    assert_eq!(serde_json::to_value(score_signal()).unwrap(), expected);
}

#[test]
fn signal_emitted_round_trips_for_every_value_kind() {
    for value in [
        SignalValue::Gauge(1.25),
        SignalValue::Counter(u64::MAX),
        SignalValue::Categorical("negative".into()),
        SignalValue::Score(0.42),
    ] {
        let mut signal = score_signal();
        signal.value = value;
        let bytes = serde_json::to_vec(&signal).unwrap();
        let back: Signal = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, signal);
        assert_eq!(back.idempotency_key(), signal.idempotency_key());
    }
}

#[test]
fn consumer_accepts_minimal_extracted_signal() {
    let wire = json!({
        "signal_id": "01J8Z3NDEKTSV4RRFFQ69G5FAV",
        "tenant_id": "11111111-2222-3333-4444-555555555555",
        "entity": {"entity_type": "email", "entity_id": "a@example.com"},
        "signal_type": "email.received",
        "ts": "2026-09-26T12:30:00Z",
        "value": {"kind": "counter", "value": 1},
        "dims": {},
        "extractor_version": "count@1",
        "config_version": "cfg-1",
        "provenance": {"source_item_ids": [], "model_version": null, "encrypted": null}
    });
    let signal: Signal = serde_json::from_value(wire).unwrap();
    assert!(signal.contributions.is_empty());
    assert_eq!(signal.validate(), Ok(()));
}

#[test]
fn consumer_rejects_message_missing_tenant_id() {
    let mut wire = serde_json::to_value(score_signal()).unwrap();
    wire.as_object_mut().unwrap().remove("tenant_id");
    assert!(serde_json::from_value::<Signal>(wire).is_err());
}
