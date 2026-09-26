use super::*;
use crate::incidents_client::IncidentDetail;
use chrono::Utc;
use common::{Incident, IncidentSeverity, IncidentStatus};

fn incident(id: Uuid, status: IncidentStatus, event_ids: Vec<Uuid>, title: &str) -> IncidentDetail {
    IncidentDetail {
        incident: Incident {
            id,
            tenant_id: Uuid::new_v4(),
            title: title.into(),
            summary: String::new(),
            severity: IncidentSeverity::Medium,
            status,
            assigned_to: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            resolved_at: None,
        },
        event_ids,
        notes: vec![],
    }
}

fn event(id: Uuid, group_key: &str, event_type: &str) -> EventSummary {
    EventSummary {
        id,
        event_type: event_type.into(),
        group_key: group_key.into(),
        status: "new".into(),
        occurred_at: Utc::now(),
        record_ids: vec![],
    }
}

#[test]
fn sweep_finds_only_unlinked_events_with_one_unambiguous_active_case() {
    let case_id = Uuid::new_v4();
    let linked_id = Uuid::new_v4();
    let candidate_id = Uuid::new_v4();
    let resolved_id = Uuid::new_v4();
    let candidates = build_correlation_candidates(
        &[
            incident(case_id, IncidentStatus::Open, vec![linked_id], "Northwind outage"),
            incident(resolved_id, IncidentStatus::Resolved, vec![resolved_id], "Closed case"),
        ],
        &[
            event(linked_id, "customer-42", "health.alert"),
            event(candidate_id, "customer-42", "health.alert.repeat"),
            event(resolved_id, "closed-key", "health.alert"),
        ],
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].event_id, candidate_id);
    assert_eq!(candidates[0].incident_id, case_id);
    assert_eq!(candidates[0].confidence, "high");
    assert!(candidates[0].evidence.iter().any(|item| item.contains("exactly one active target")));
}

#[test]
fn sweep_skips_ambiguous_keys_and_blank_group_keys() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let first_case = Uuid::new_v4();
    let second_case = Uuid::new_v4();
    let ambiguous = Uuid::new_v4();
    let blank = Uuid::new_v4();
    let candidates = build_correlation_candidates(
        &[
            incident(first_case, IncidentStatus::Open, vec![first], "First"),
            incident(second_case, IncidentStatus::Open, vec![second], "Second"),
        ],
        &[
            event(first, "shared-key", "one"),
            event(second, "shared-key", "two"),
            event(ambiguous, "shared-key", "three"),
            event(blank, "", "four"),
        ],
    );

    assert!(candidates.is_empty());
}

#[test]
fn ambiguous_groups_expose_active_targets_without_guessing_one() {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let pending = Uuid::new_v4();
    let first_case = Uuid::new_v4();
    let second_case = Uuid::new_v4();
    let groups = build_ambiguous_correlation_groups(
        &[
            incident(first_case, IncidentStatus::Open, vec![first], "First"),
            incident(second_case, IncidentStatus::Acknowledged, vec![second], "Second"),
        ],
        &[
            event(first, "shared-key", "one"),
            event(second, "shared-key", "two"),
            event(pending, "shared-key", "three"),
        ],
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].group_key, "shared-key");
    assert_eq!(groups[0].event_ids, vec![pending]);
    assert_eq!(groups[0].incident_options.len(), 2);
    assert!(groups[0].incident_options.iter().any(|option| option.id == first_case));
    assert!(groups[0].incident_options.iter().any(|option| option.id == second_case));
}

#[test]
fn correlation_sweep_is_exposed_as_an_operator_control_surface() {
    let queue = include_str!("../templates/incidents.html");
    let sweep = include_str!("../templates/incident_correlation_sweep.html");
    assert!(queue.contains("/incidents/correlation-sweep"));
    assert!(queue.contains("Run correlation sweep"));
    assert!(sweep.contains("Safe matches are revalidated at submission."));
    assert!(sweep.contains("name=\"event_ids\""));
    assert!(sweep.contains("Ambiguous groups"));
    assert!(sweep.contains("Auto-link all safe matches"));
    assert!(sweep.contains("/incidents/correlation-sweep/resolve"));
    assert!(sweep.contains("Resolve ambiguity"));
    assert!(sweep.contains("Confidence / evidence"));
    assert!(sweep.contains("candidate.confidence"));
    assert!(sweep.contains("candidate.evidence"));
}

#[test]
fn correlation_sweep_exposes_a_versioned_machine_api() {
    let source = include_str!("incident_correlation_handler.rs");
    assert!(source.contains("pub async fn get_correlation_sweep_api"));
    assert!(source.contains("pub async fn post_correlation_sweep_api"));
    assert!(source.contains("pub async fn post_correlation_sweep_auto_api"));
    assert!(source.contains("rejected_count"));
    assert!(source.contains("ambiguous_groups"));
}
