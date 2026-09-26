use super::*;
use serde_json::json;

#[test]
fn kind_matches_serialized_tag_for_every_variant() {
    let values = [
        SignalValue::Gauge(1.5),
        SignalValue::Counter(3),
        SignalValue::Categorical("negative".into()),
        SignalValue::Score(0.25),
    ];
    let expected = ["gauge", "counter", "categorical", "score"];
    for (value, kind) in values.iter().zip(expected) {
        assert_eq!(value.kind(), kind);
        assert_eq!(serde_json::to_value(value).unwrap()["kind"], kind);
    }
}

#[test]
fn serializes_adjacently_tagged() {
    assert_eq!(
        serde_json::to_value(SignalValue::Gauge(1.5)).unwrap(),
        json!({"kind": "gauge", "value": 1.5})
    );
    assert_eq!(
        serde_json::to_value(SignalValue::Counter(7)).unwrap(),
        json!({"kind": "counter", "value": 7})
    );
    assert_eq!(
        serde_json::to_value(SignalValue::Categorical("x".into())).unwrap(),
        json!({"kind": "categorical", "value": "x"})
    );
}

#[test]
fn counter_rejects_negative_on_deserialize() {
    let err = serde_json::from_value::<SignalValue>(json!({"kind": "counter", "value": -1}));
    assert!(err.is_err());
}

#[test]
fn unknown_kind_is_rejected() {
    assert!(
        serde_json::from_value::<SignalValue>(json!({"kind": "histogram", "value": 1})).is_err()
    );
}
