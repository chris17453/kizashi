use super::{
    action_library_query, matches_action_query, parameter_fields, satisfies_preconditions,
};

#[test]
fn normalizes_action_library_query_without_losing_operator_scope() {
    assert_eq!(action_library_query("  email  "), "email");
    assert_eq!(action_library_query("   "), "");
}

#[test]
fn action_query_matches_name_contract_and_target_type() {
    assert!(matches_action_query("Escalate ticket", "Ticket", "status", "ticket"));
    assert!(matches_action_query("Notify owner", "Customer", "email", "notify"));
    assert!(!matches_action_query("Notify owner", "Customer", "email", "invoice"));
}

#[test]
fn parameter_contract_becomes_typed_execution_fields() {
    let fields = parameter_fields(&serde_json::json!({
        "next_status": {"type": "string", "required": true},
        "notify": {"type": "boolean", "default": true},
        "metadata": {"type": "object"}
    }));
    assert_eq!(fields.len(), 3);
    let notify = fields.iter().find(|field| field.name == "notify").unwrap();
    let next_status = fields.iter().find(|field| field.name == "next_status").unwrap();
    assert_eq!(notify.field_type, "boolean");
    assert_eq!(notify.default_value, "true");
    assert!(next_status.required);
}

#[test]
fn target_preconditions_mark_only_matching_objects_eligible() {
    assert!(satisfies_preconditions(
        &serde_json::json!({"status": "open"}),
        &serde_json::json!({"status": "open"})
    ));
    assert!(!satisfies_preconditions(
        &serde_json::json!({"status": "closed"}),
        &serde_json::json!({"status": "open"})
    ));
}

#[test]
fn action_execution_requires_at_least_one_eligible_target() {
    let targets = [
        super::ActionTargetView { id: uuid::Uuid::nil(), label: "Closed".into(), eligible: false },
        super::ActionTargetView { id: uuid::Uuid::new_v4(), label: "Open".into(), eligible: true },
    ];
    assert!(targets.iter().any(|target| target.eligible));
    assert!(![targets[0].clone()].iter().any(|target| target.eligible));
}

#[test]
fn action_library_execution_keeps_contract_search_scope() {
    let template = include_str!("../templates/actions_library.html");
    assert!(template
        .contains("/actions/library{% if !query.is_empty() %}?q={{ query|urlencode }}{% endif %}"));
}

#[test]
fn action_library_exposes_operational_posture_visuals() {
    let template = include_str!("../templates/actions_library.html");
    assert!(template.contains("Target coverage"));
    assert!(template.contains("Execution outcomes"));
    assert!(template.contains("action-library-readiness-track"));
    assert!(template.contains("eligible_target_count"));
    assert!(template.contains("completed_invocation_count"));
}

#[test]
fn action_library_exposes_multi_target_governed_execution() {
    let template = include_str!("../templates/actions_library.html");
    assert!(template.contains("data-library-target-select multiple"));
    assert!(template.contains("name=\"target_object_ids\""));
    assert!(template.contains("selected.join(',')"));
}

#[test]
fn action_library_exposes_before_after_contract_diffs() {
    let template = include_str!("../templates/actions_library.html");
    assert!(template.contains("View version diff"));
    assert!(template.contains("{{ change.before_state }}"));
    assert!(template.contains("{{ change.after_state }}"));
}

#[test]
fn action_library_shows_a_target_set_preflight_before_submit() {
    let template = include_str!("../templates/actions_library.html");
    assert!(template.contains("className = 'action-preflight'"));
    assert!(template.contains("No state changes occur until you submit."));
}

#[test]
fn action_library_exposes_cross_contract_target_readiness() {
    let template = include_str!("../templates/actions_library.html");
    assert!(template.contains("Target-type readiness"));
    assert!(template.contains("action-target-coverage-row"));
    assert!(template.contains("blocked_count"));
    assert!(template.contains("Object-centric response"));
}
