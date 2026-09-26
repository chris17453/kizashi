use super::*;
use crate::incident_repository::incident_repository_test::InMemoryIncidentRepository;
use crate::IncidentRepository;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn blank_group_keys_are_ignored_without_touching_the_repository() {
    let repository: Arc<dyn crate::IncidentRepository> =
        Arc::new(InMemoryIncidentRepository::default());
    let event = common::Event::new(
        Uuid::new_v4(),
        "health.degraded",
        "service-a",
        "   ",
        serde_json::json!({}),
        Utc::now(),
    );
    assert_eq!(correlate_event(&repository, &event).await.unwrap(), CorrelationOutcome::Ignored);
}

#[tokio::test]
async fn one_active_matching_case_receives_the_event_context() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let incident = common::Incident {
        id: Uuid::new_v4(),
        tenant_id,
        title: "Checkout degradation".to_string(),
        summary: String::new(),
        severity: common::IncidentSeverity::High,
        status: common::IncidentStatus::Open,
        assigned_to: None,
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };
    let repository = Arc::new(InMemoryIncidentRepository::with_incident(incident.clone()));
    repository
        .link_event_with_context(
            tenant_id,
            incident.id,
            Uuid::new_v4(),
            Some("checkout"),
            "operator",
        )
        .await
        .unwrap();
    let event = common::Event::new(
        tenant_id,
        "health.degraded",
        "checkout",
        "checkout",
        serde_json::json!({}),
        Utc::now(),
    );

    assert_eq!(
        correlate_event(&(repository.clone() as Arc<dyn crate::IncidentRepository>), &event)
            .await
            .unwrap(),
        CorrelationOutcome::Linked(incident.id)
    );
    assert!(repository.links.lock().unwrap().iter().any(|(_, id)| *id == event.id));
}

#[tokio::test]
async fn one_active_same_entity_and_event_type_receives_a_partial_duplicate() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let incident = common::Incident {
        id: Uuid::new_v4(),
        tenant_id,
        title: "Checkout degradation".to_string(),
        summary: String::new(),
        severity: common::IncidentSeverity::High,
        status: common::IncidentStatus::Open,
        assigned_to: None,
        created_at: now,
        updated_at: now,
        resolved_at: None,
    };
    let repository = Arc::new(InMemoryIncidentRepository::with_incident(incident.clone()));
    repository
        .link_event_with_identity(
            tenant_id,
            incident.id,
            Uuid::new_v4(),
            Some("checkout:attempt-1"),
            crate::incident_repository::EventCorrelationIdentity {
                event_type: "health.degraded",
                entity_ref: "checkout-service",
            },
            "operator",
        )
        .await
        .unwrap();
    let event = common::Event::new(
        tenant_id,
        "health.degraded",
        "checkout-service",
        "checkout:attempt-2",
        serde_json::json!({}),
        Utc::now(),
    );

    assert_eq!(
        correlate_event(&(repository.clone() as Arc<dyn crate::IncidentRepository>), &event)
            .await
            .unwrap(),
        CorrelationOutcome::PartialLinked(incident.id)
    );
    assert!(repository.links.lock().unwrap().iter().any(|(_, id)| *id == event.id));
}
