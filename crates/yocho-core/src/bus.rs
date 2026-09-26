//! Yochō bus topic names, following the `<noun>.<verb>` convention of `common::bus`. Each
//! `signal.emitted` message body is one JSON-serialized [`crate::Signal`]; the shape is pinned
//! by `tests/signal_emitted_contract_test.rs`.

/// Exchange every Yochō producer (extractor, detector, scorer) publishes signals on; the
/// signal writer, detectors and scorer consume it (docs/yocho.md "Pipeline").
pub const SIGNAL_EMITTED_EXCHANGE: &str = "signal.emitted";
