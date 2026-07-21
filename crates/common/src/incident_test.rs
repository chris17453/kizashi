use super::*;
use std::str::FromStr;

#[test]
fn severity_round_trips_through_display_and_from_str() {
    for severity in [
        IncidentSeverity::Low,
        IncidentSeverity::Medium,
        IncidentSeverity::High,
        IncidentSeverity::Critical,
    ] {
        assert_eq!(IncidentSeverity::from_str(&severity.to_string()).unwrap(), severity);
    }
}

#[test]
fn severity_from_str_rejects_unknown_values() {
    assert!(IncidentSeverity::from_str("catastrophic").is_err());
}

#[test]
fn severity_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&IncidentSeverity::High).unwrap(), "\"high\"");
}

#[test]
fn status_round_trips_through_display_and_from_str() {
    for status in [IncidentStatus::Open, IncidentStatus::Acknowledged, IncidentStatus::Resolved] {
        assert_eq!(IncidentStatus::from_str(&status.to_string()).unwrap(), status);
    }
}

#[test]
fn status_from_str_rejects_unknown_values() {
    assert!(IncidentStatus::from_str("stale").is_err());
}

#[test]
fn status_serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&IncidentStatus::Acknowledged).unwrap(), "\"acknowledged\"");
}

fn sample_incident() -> Incident {
    Incident {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        title: "elevated error rate".to_string(),
        summary: String::new(),
        severity: IncidentSeverity::High,
        status: IncidentStatus::Open,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        resolved_at: None,
    }
}

#[test]
fn incident_round_trips_through_json() {
    let incident = sample_incident();
    let json = serde_json::to_string(&incident).unwrap();
    let back: Incident = serde_json::from_str(&json).unwrap();
    assert_eq!(incident, back);
}
