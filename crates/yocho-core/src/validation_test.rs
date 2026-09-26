use super::*;
use crate::provenance::Contribution;
use crate::signal_id::SignalId;
use crate::signal_value::SignalValue;
use crate::EntityRef;
use chrono::{TimeZone, Utc};
use proptest::prelude::*;
use std::collections::BTreeMap;
use uuid::Uuid;

type Mutation = fn(&mut Signal);

fn valid() -> Signal {
    Signal {
        signal_id: SignalId::new(),
        tenant_id: Uuid::nil(),
        entity: EntityRef::new("domain", "example.com"),
        signal_type: "ticket.opened".into(),
        ts: Utc.with_ymd_and_hms(2026, 9, 26, 0, 0, 0).unwrap(),
        value: SignalValue::Counter(2),
        dims: BTreeMap::new(),
        extractor_version: "zendesk-tickets@1".into(),
        config_version: "cfg-1".into(),
        provenance: Default::default(),
        contributions: vec![],
    }
}

#[test]
fn valid_signal_passes() {
    assert_eq!(valid().validate(), Ok(()));
}

#[test]
fn empty_identifiers_are_rejected() {
    let cases: Vec<(Mutation, &str)> = vec![
        (|s| s.signal_type.clear(), "signal_type"),
        (|s| s.entity.entity_type.clear(), "entity_type"),
        (|s| s.entity.entity_id.clear(), "entity_id"),
        (|s| s.extractor_version.clear(), "extractor_version"),
        (|s| s.config_version.clear(), "config_version"),
        (|s| s.dims = BTreeMap::from([(String::new(), "v".into())]), "dims key"),
    ];
    for (mutate, field) in cases {
        let mut s = valid();
        mutate(&mut s);
        assert_eq!(s.validate(), Err(SignalError::Empty { field }));
    }
}

#[test]
fn whitespace_only_identifiers_count_as_empty() {
    let mut s = valid();
    s.entity.entity_id = "  \t".into();
    assert_eq!(s.validate(), Err(SignalError::Empty { field: "entity_id" }));
}

#[test]
fn overlong_fields_are_rejected() {
    let long = "x".repeat(MAX_IDENT_LEN + 1);
    let mut s = valid();
    s.signal_type = long.clone();
    assert_eq!(
        s.validate(),
        Err(SignalError::TooLong { field: "signal_type", max: MAX_IDENT_LEN })
    );

    let mut s = valid();
    s.value = SignalValue::Categorical(long);
    assert_eq!(s.validate(), Err(SignalError::TooLong { field: "value", max: MAX_IDENT_LEN }));

    let mut s = valid();
    s.dims.insert("k".repeat(MAX_DIM_KEY_LEN + 1), "v".into());
    assert_eq!(s.validate(), Err(SignalError::TooLong { field: "dims key", max: MAX_DIM_KEY_LEN }));

    let mut s = valid();
    s.dims.insert("k".into(), "v".repeat(MAX_DIM_VALUE_LEN + 1));
    let expected = SignalError::TooLong { field: "dims value", max: MAX_DIM_VALUE_LEN };
    assert_eq!(s.validate(), Err(expected));
}

#[test]
fn fields_at_the_limit_are_accepted() {
    let mut s = valid();
    s.signal_type = "x".repeat(MAX_IDENT_LEN);
    s.dims.insert("k".repeat(MAX_DIM_KEY_LEN), "v".repeat(MAX_DIM_VALUE_LEN));
    assert_eq!(s.validate(), Ok(()));
}

#[test]
fn too_many_dims_are_rejected() {
    let mut s = valid();
    s.dims = (0..=MAX_DIMS).map(|i| (format!("k{i}"), "v".to_string())).collect();
    assert_eq!(s.validate(), Err(SignalError::TooManyDims { count: MAX_DIMS + 1, max: MAX_DIMS }));
}

#[test]
fn empty_categorical_is_rejected() {
    let mut s = valid();
    s.value = SignalValue::Categorical(String::new());
    assert_eq!(s.validate(), Err(SignalError::Empty { field: "value" }));
}

#[test]
fn non_finite_values_are_rejected() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for value in [SignalValue::Gauge(bad), SignalValue::Score(bad)] {
            let mut s = valid();
            s.value = value;
            assert_eq!(s.validate(), Err(SignalError::NotFinite { field: "value" }));
        }
        let mut s = valid();
        s.contributions =
            vec![Contribution { signal_id: SignalId::new(), weight: bad, value: 1.0 }];
        assert_eq!(s.validate(), Err(SignalError::NotFinite { field: "contribution weight" }));
        let mut s = valid();
        s.contributions =
            vec![Contribution { signal_id: SignalId::new(), weight: 1.0, value: bad }];
        assert_eq!(s.validate(), Err(SignalError::NotFinite { field: "contribution value" }));
    }
}

#[test]
fn empty_source_item_id_is_rejected() {
    let mut s = valid();
    s.provenance.source_item_ids = vec!["ok".into(), String::new()];
    assert_eq!(s.validate(), Err(SignalError::Empty { field: "source_item_id" }));
}

#[test]
fn blank_model_version_is_rejected_but_absent_is_fine() {
    let mut s = valid();
    s.provenance.model_version = Some(" ".into());
    assert_eq!(s.validate(), Err(SignalError::Empty { field: "model_version" }));
    s.provenance.model_version = Some("gbm@1".into());
    assert_eq!(s.validate(), Ok(()));
}

#[test]
fn error_messages_name_the_field() {
    assert_eq!(
        SignalError::Empty { field: "entity_id" }.to_string(),
        "`entity_id` must not be empty"
    );
    assert!(SignalError::TooManyDims { count: 40, max: 32 }.to_string().contains("40"));
}

fn any_value() -> impl Strategy<Value = SignalValue> {
    prop_oneof![
        any::<f64>().prop_map(SignalValue::Gauge),
        any::<u64>().prop_map(SignalValue::Counter),
        ".{0,300}".prop_map(SignalValue::Categorical),
        any::<f64>().prop_map(SignalValue::Score),
    ]
}

proptest! {
    #[test]
    fn validate_never_panics_and_accepts_only_finite_bounded_signals(
        signal_type in ".{0,300}",
        entity_type in ".{0,300}",
        entity_id in ".{0,300}",
        value in any_value(),
        dims in proptest::collection::btree_map(".{0,80}", ".{0,300}", 0..40),
        weights in proptest::collection::vec((any::<f64>(), any::<f64>()), 0..4),
        source_ids in proptest::collection::vec(".{0,20}", 0..4),
    ) {
        let mut s = valid();
        s.signal_type = signal_type;
        s.entity = EntityRef::new(entity_type, entity_id);
        s.value = value;
        s.dims = dims;
        s.contributions = weights
            .into_iter()
            .map(|(weight, value)| Contribution { signal_id: SignalId::new(), weight, value })
            .collect();
        s.provenance.source_item_ids = source_ids;
        let result = s.validate();
        let _ = s.idempotency_key();
        if result.is_ok() {
            match &s.value {
                SignalValue::Gauge(v) | SignalValue::Score(v) => prop_assert!(v.is_finite()),
                SignalValue::Categorical(c) => prop_assert!(!c.trim().is_empty()),
                SignalValue::Counter(_) => {}
            }
            prop_assert!(s.dims.len() <= MAX_DIMS);
            prop_assert!(!s.signal_type.trim().is_empty());
            prop_assert!(s.contributions.iter().all(|c| c.weight.is_finite() && c.value.is_finite()));
        }
    }
}
