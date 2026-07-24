use super::{build_attention_summary, unique_attention_case_count};
use crate::backlog_client::QueueDepthSummary;
use crate::incidents_client::IncidentDetail;
use chrono::Utc;
use common::ontology::ActionInvocation;
use common::{Incident, IncidentSeverity, IncidentStatus};
use serde_json::json;
use uuid::Uuid;

#[test]
fn attention_summary_counts_operational_pressure() {
    let now = Utc::now();
    let incident = |severity, assigned_to| IncidentDetail {
        incident: Incident {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            title: "case".into(),
            summary: String::new(),
            severity,
            status: IncidentStatus::Open,
            assigned_to,
            created_at: now,
            updated_at: now,
            resolved_at: None,
        },
        event_ids: vec![],
        notes: vec![],
    };
    let action = ActionInvocation {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        action_type_id: Uuid::new_v4(),
        target_object_ids: json!([]),
        parameters: json!({}),
        outcome: "Rejected".into(),
        triggering_event_ref: json!({}),
        contract_snapshot: None,
        executed_at: now,
    };
    let summary = build_attention_summary(
        &[
            incident(IncidentSeverity::Critical, None),
            incident(IncidentSeverity::Low, Some("operator".into())),
        ],
        &[action],
        &[QueueDepthSummary { stage: "event".into(), queue_name: "events".into(), messages: 50 }],
        0,
    );
    assert_eq!(summary.open_incidents, 2);
    assert_eq!(summary.critical_incidents, 1);
    assert_eq!(summary.unassigned_incidents, 1);
    assert_eq!(summary.review_actions, 1);
    assert_eq!(summary.critical_queues, 1);
    assert_eq!(summary.sla_breaches, 0);
    // The critical case is also unassigned; command posture counts that case once, then adds
    // the independent review action and critical queue signals.
    assert_eq!(summary.attention_count, 3);
}

#[test]
fn attention_summary_routes_breached_sla_cases() {
    let now = Utc::now();
    let incident = IncidentDetail {
        incident: Incident {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            title: "overdue case".into(),
            summary: String::new(),
            severity: IncidentSeverity::High,
            status: IncidentStatus::Open,
            assigned_to: Some("operator".into()),
            created_at: now - chrono::Duration::hours(2),
            updated_at: now,
            resolved_at: None,
        },
        event_ids: vec![],
        notes: vec![],
    };
    let summary = build_attention_summary(&[incident], &[], &[], 0);
    assert_eq!(summary.sla_breaches, 1);
    assert_eq!(summary.attention_count, 1);
}

#[test]
fn unique_attention_case_count_deduplicates_overlapping_pressure_signals() {
    let now = Utc::now();
    let case = IncidentDetail {
        incident: Incident {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            title: "critical unassigned case".into(),
            summary: String::new(),
            severity: IncidentSeverity::Critical,
            status: IncidentStatus::Open,
            assigned_to: None,
            created_at: now - chrono::Duration::hours(2),
            updated_at: now,
            resolved_at: None,
        },
        event_ids: vec![],
        notes: vec![],
    };
    assert_eq!(unique_attention_case_count(&[case], now), 1);
}
