use super::*;
use crate::provenance::{Contribution, Provenance};
use crate::signal_value::SignalValue;
use crate::{EntityRef, SignalId};
use common::SourceType;
use serde_json::json;
use std::collections::BTreeMap;

fn signal_from(item: &RawRecord, value: SignalValue) -> Signal {
    Signal {
        signal_id: SignalId::new(),
        tenant_id: item.tenant_id,
        entity: EntityRef::new("email", "a@example.com"),
        signal_type: "email.received".into(),
        ts: item.ingested_at,
        value,
        dims: BTreeMap::new(),
        extractor_version: "count@1".into(),
        config_version: "cfg-1".into(),
        provenance: Provenance { source_item_ids: vec![item.id.to_string()], ..Default::default() },
        contributions: vec![],
    }
}

struct CountExtractor;

#[async_trait]
impl Extractor for CountExtractor {
    fn name(&self) -> &str {
        "count"
    }
    fn version(&self) -> &str {
        "count@1"
    }
    async fn extract(&self, item: &RawRecord) -> Result<Vec<Signal>, StageError> {
        if item.source_type != SourceType::Message {
            return Err(StageError::UnsupportedInput("not a message".into()));
        }
        let signal = signal_from(item, SignalValue::Counter(1));
        signal.validate()?;
        Ok(vec![signal])
    }
}

struct SpikeDetector;

#[async_trait]
impl Detector for SpikeDetector {
    fn name(&self) -> &str {
        "spike"
    }
    fn version(&self) -> &str {
        "spike@1"
    }
    async fn detect(&self, window: &[Signal]) -> Result<Vec<Signal>, StageError> {
        Ok(window.iter().take(1).cloned().collect())
    }
}

struct SumScorer;

#[async_trait]
impl Scorer for SumScorer {
    fn name(&self) -> &str {
        "sum"
    }
    fn version(&self) -> &str {
        "sum@1"
    }
    async fn score(&self, inputs: &[Signal]) -> Result<Signal, StageError> {
        let first = inputs.first().ok_or(StageError::Unavailable("no inputs".into()))?;
        let mut out = first.clone();
        out.signal_id = SignalId::new();
        out.value = SignalValue::Score(inputs.len() as f64);
        out.contributions = inputs
            .iter()
            .map(|s| Contribution { signal_id: s.signal_id, weight: 1.0, value: 1.0 })
            .collect();
        Ok(out)
    }
}

fn message() -> RawRecord {
    RawRecord::new("graph:mail", SourceType::Message, uuid::Uuid::new_v4(), json!({}))
}

#[tokio::test]
async fn stages_are_object_safe_and_chain() {
    let extractor: Box<dyn Extractor> = Box::new(CountExtractor);
    let detector: Box<dyn Detector> = Box::new(SpikeDetector);
    let scorer: Box<dyn Scorer> = Box::new(SumScorer);
    assert_eq!((extractor.name(), extractor.version()), ("count", "count@1"));
    assert_eq!((detector.name(), detector.version()), ("spike", "spike@1"));
    assert_eq!((scorer.name(), scorer.version()), ("sum", "sum@1"));

    let signals = extractor.extract(&message()).await.unwrap();
    let flagged = detector.detect(&signals).await.unwrap();
    let score = scorer.score(&flagged).await.unwrap();

    assert_eq!(score.value, SignalValue::Score(1.0));
    assert_eq!(score.contributions[0].signal_id, signals[0].signal_id);
    assert_eq!(score.validate(), Ok(()));
}

#[tokio::test]
async fn stage_errors_surface_with_context() {
    let generic = RawRecord::new("generic", SourceType::Generic, uuid::Uuid::new_v4(), json!({}));
    let err = CountExtractor.extract(&generic).await.unwrap_err();
    assert_eq!(err.to_string(), "unsupported input: not a message");

    let err = SumScorer.score(&[]).await.unwrap_err();
    assert_eq!(err.to_string(), "dependency unavailable: no inputs");
}

#[test]
fn signal_errors_convert_into_stage_errors() {
    let err: StageError = SignalError::Empty { field: "entity_id" }.into();
    assert!(matches!(err, StageError::InvalidSignal(_)));
    assert_eq!(err.to_string(), "invalid signal: `entity_id` must not be empty");
}
