use super::{normalize_event_coverage, EventTypesTemplate};
use askama::Template;
use uuid::Uuid;

#[test]
fn renders_operator_controls_for_live_contract_refresh() {
    let body = EventTypesTemplate {
        show_nav: true,
        is_admin: false,
        tenant_id: Uuid::new_v4(),
        username: "operator".to_string(),
        event_types: vec![],
        total_events: 0,
        error: None,
        q: "risk".to_string(),
        notice: String::new(),
        can_write: true,
        governed_count: 0,
        triggerless_count: 0,
        observed_only_count: 0,
        activity_bars: vec![],
        coverage_scope: String::new(),
    }
    .render()
    .unwrap();
    assert!(body.contains("data-event-types-live-status"));
    assert!(body.contains("data-event-types-refresh"));
    assert!(body.contains("data-event-types-toggle-live"));
    assert!(body.contains("kizashi.event-types.live-refresh"));
    assert!(body.contains("data-event-type-query"));
}

#[test]
fn event_coverage_scope_accepts_governance_blind_spots() {
    assert_eq!(normalize_event_coverage("TRIGGERLESS"), "triggerless");
    assert_eq!(normalize_event_coverage("observed_only"), "observed_only");
    assert!(normalize_event_coverage("other").is_empty());
}

#[test]
fn event_type_surface_exposes_volume_and_detection_coverage_map() {
    let template = include_str!("../templates/event_types.html");
    assert!(template.contains("Contract coverage map"));
    assert!(template.contains("event-coverage-fill"));
    assert!(template.contains("Schema gap"));
    assert!(template.contains("No trigger"));
    assert!(template.contains("/events?q={{ item.name|urlencode }}"));
}
