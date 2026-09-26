use super::*;

#[test]
fn model_module_exports_the_safe_and_ambiguous_review_contracts() {
    assert!(std::mem::size_of::<CorrelationCandidate>() > 0);
    assert!(std::mem::size_of::<AmbiguousCorrelationGroup>() > 0);
}
