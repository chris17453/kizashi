use super::*;
use uuid::Uuid;

#[test]
fn new_sets_prompt_and_updated_at() {
    let tenant_id = Uuid::new_v4();
    let config = AnalysisConfig::new(tenant_id, "look for urgent tickets");

    assert_eq!(config.tenant_id, tenant_id);
    assert_eq!(config.prompt, "look for urgent tickets");
}

#[test]
fn round_trips_through_json() {
    let config = AnalysisConfig::new(Uuid::new_v4(), "flag policy violations");
    let json = serde_json::to_string(&config).unwrap();
    let back: AnalysisConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, back);
}
