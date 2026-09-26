use super::*;

#[test]
fn only_terminal_review_decisions_close_a_case() {
    assert!(can_decide(WorkflowCaseStatus::Approved));
    assert!(can_decide(WorkflowCaseStatus::Rejected));
    assert!(!can_decide(WorkflowCaseStatus::Exception));
}
