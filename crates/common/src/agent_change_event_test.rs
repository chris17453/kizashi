use super::*;
use crate::Agent;
use uuid::Uuid;

fn sample_agent() -> Agent {
    Agent::new(Uuid::new_v4(), "zendesk", "support-poller", serde_json::json!({}))
}

#[test]
fn upserted_round_trips_through_json() {
    let event = AgentChangeEvent::Upserted(sample_agent());
    let json = serde_json::to_string(&event).unwrap();
    let back: AgentChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}

#[test]
fn deleted_round_trips_through_json() {
    let event = AgentChangeEvent::Deleted { id: Uuid::new_v4(), tenant_id: Uuid::new_v4() };
    let json = serde_json::to_string(&event).unwrap();
    let back: AgentChangeEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, back);
}
