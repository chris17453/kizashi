use super::*;

#[test]
fn workflow_status_serializes_as_a_stable_queue_value() {
    assert_eq!(
        serde_json::to_string(&WorkflowCaseStatus::PendingReview).unwrap(),
        "\"pending_review\""
    );
}
