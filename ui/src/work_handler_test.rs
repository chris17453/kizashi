use super::{
    csv_escape, matches_age, matches_work_text, normalize_age, normalize_focus, normalize_severity,
    normalize_sla, saved_work_views, work_view_redirect, SaveWorkViewForm, WorkAction,
    WorkIncident,
};
use axum::response::IntoResponse;
use common::SavedSearchQuery;
use uuid::Uuid;

#[test]
fn work_items_keep_deep_links_as_stable_identifiers() {
    let incident = WorkIncident {
        id: Uuid::nil(),
        title: "Case".into(),
        severity: "high".into(),
        status: "open".into(),
        owner: Some("alice".into()),
        event_count: 2,
        sla_state: "at-risk".into(),
        sla_label: "At risk".into(),
        sla_detail: "10m remaining".into(),
        age_days: 4,
    };
    let action = WorkAction {
        id: Uuid::nil(),
        action_type_id: Uuid::nil(),
        target_object_ids: Uuid::nil().to_string(),
        parameters: "{}".into(),
        name: "Escalate".into(),
        outcome: "Rejected".into(),
        review_status: "handed off".into(),
        review_assignee: "operator".into(),
        executed_at: chrono::Utc::now(),
        incident_id: Some(incident.id),
        event_id: None,
        targets: Vec::new(),
    };
    assert_eq!(incident.event_count, 2);
    assert_eq!(action.incident_id, Some(Uuid::nil()));
    assert!(action.event_id.is_none());
}

#[test]
fn work_focus_only_accepts_shareable_queue_names() {
    assert_eq!(normalize_focus("assigned"), "assigned");
    assert_eq!(normalize_focus("unassigned"), "unassigned");
    assert_eq!(normalize_focus("review"), "review");
    assert_eq!(normalize_focus("anything-else"), "");
}

#[test]
fn work_filters_normalize_severity_and_match_case_insensitively() {
    assert_eq!(normalize_severity("critical"), "critical");
    assert_eq!(normalize_severity("unknown"), "");
    assert!(matches_work_text("Critical SSO incident", "sso"));
    assert!(!matches_work_text("Critical SSO incident", "latency"));
}

#[test]
fn work_sla_filter_accepts_only_operational_postures() {
    assert_eq!(normalize_sla("breached"), "breached");
    assert_eq!(normalize_sla("at-risk"), "at-risk");
    assert_eq!(normalize_sla("on-track"), "on-track");
    assert_eq!(normalize_sla("anything-else"), "");
}

#[test]
fn work_age_filter_partitions_active_cases_without_overlap() {
    assert!(matches_age(0, "0_1"));
    assert!(matches_age(1, "1_7"));
    assert!(matches_age(7, "1_7"));
    assert!(matches_age(8, "8_30"));
    assert!(matches_age(30, "8_30"));
    assert!(matches_age(31, "31_plus"));
    assert!(!matches_age(8, "1_7"));
    assert_eq!(normalize_age("invalid"), "");
}

#[test]
fn work_csv_escape_quotes_handoff_values() {
    assert_eq!(csv_escape("Northwind, Inc."), "\"Northwind, Inc.\"");
    assert_eq!(csv_escape("line\nwrap"), "\"line\nwrap\"");
}

#[test]
fn saved_work_views_round_trip_only_work_filters() {
    let query = SavedSearchQuery::new(
        Uuid::new_v4(),
        "High unassigned",
        serde_json::json!({"view_kind":"work","focus":"unassigned","severity":"high","sla":"breached","q":"access"}),
    );
    let views = saved_work_views(vec![query]);
    assert_eq!(views.len(), 1);
    assert!(views[0].load_url.contains("focus=unassigned"));
    assert!(views[0].load_url.contains("severity=high"));
    assert!(views[0].load_url.contains("sla=breached"));
    assert!(views[0].load_url.contains("q=access"));
}

#[test]
fn work_queue_surfaces_sla_posture_on_each_case() {
    let template = include_str!("../templates/work.html");
    assert!(template.contains("sla-{{ item.sla_state }}"));
    assert!(template.contains("{{ item.sla_label }}"));
    assert!(template.contains("{{ item.sla_detail }}"));
}

#[test]
fn work_review_actions_link_to_modeled_targets() {
    let source = include_str!("work_handler.rs");
    assert!(source.contains("struct WorkActionTarget"));
    let template = include_str!("../templates/work.html");
    assert!(template.contains("Targets:"));
    assert!(template.contains("/ontology?object_id={{ target.id }}#object-{{ target.id }}"));
}

#[test]
fn work_focus_navigation_preserves_active_filters() {
    let template = include_str!("../templates/work.html");
    assert!(template.contains("focus=assigned&q={{ q|urlencode }}&severity={{ severity|urlencode }}&sla={{ sla|urlencode }}&age={{ age|urlencode }}"));
    assert!(template.contains("focus=unassigned&q={{ q|urlencode }}&severity={{ severity|urlencode }}&sla={{ sla|urlencode }}&age={{ age|urlencode }}"));
    assert!(template.contains("focus=review&q={{ q|urlencode }}&severity={{ severity|urlencode }}&sla={{ sla|urlencode }}&age={{ age|urlencode }}"));
}

#[test]
fn work_view_save_preserves_the_active_scope() {
    let form = SaveWorkViewForm {
        name: "High unassigned".to_string(),
        q: "Northwind".to_string(),
        severity: "high".to_string(),
        sla: "breached".to_string(),
        age: "8_30".to_string(),
        focus: "unassigned".to_string(),
    };
    let response = work_view_redirect(&form, "view_saved").into_response();
    let location = response.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("q=Northwind"));
    assert!(location.contains("severity=high"));
    assert!(location.contains("sla=breached"));
    assert!(location.contains("age=8_30"));
    assert!(location.contains("focus=unassigned"));
    assert!(location.contains("notice=view_saved"));
}

#[test]
fn bulk_claim_form_preserves_focus_scope() {
    let template = include_str!("../templates/work.html");
    assert!(template.contains("name=\"focus\" value=\"{{ focus }}\" form=\"bulk-claim-form\""));
    assert!(template.contains("form[action*=\"/claim\"]"));
}

#[test]
fn bulk_claim_exposes_ownership_impact_preflight() {
    let template = include_str!("../templates/work.html");
    assert!(template.contains("id=\"work-claim-preflight\""));
    assert!(template.contains("data-work-severity=\"{{ item.severity }}\""));
    assert!(template.contains("data-work-sla=\"{{ item.sla_state }}\""));
    assert!(template.contains("Ownership changes are audited per case."));
}
