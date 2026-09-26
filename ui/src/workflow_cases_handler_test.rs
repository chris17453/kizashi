use super::*;
#[test]
fn workflow_decision_form_only_accepts_terminal_decisions() {
    assert!(parse_decision("approved").is_some());
    assert!(parse_decision("exception").is_none());
}
#[test]
fn workflow_template_offers_operator_decisions() {
    let template = include_str!("../templates/workflow_cases.html");
    assert!(
        template.contains("approved")
            && template.contains("rejected")
            && template.contains("resolved")
    );
}
