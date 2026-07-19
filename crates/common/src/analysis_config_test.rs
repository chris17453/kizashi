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
fn new_defaults_to_azure_foundry_provider_with_no_overrides() {
    let config = AnalysisConfig::new(Uuid::new_v4(), "look for urgent tickets");

    assert_eq!(config.provider, AnalysisProvider::AzureFoundry);
    assert!(config.model.is_none());
    assert!(config.endpoint.is_none());
    assert!(config.api_key.is_none());
}

#[test]
fn deserializing_a_config_without_provider_fields_defaults_to_azure_foundry() {
    // Proves ADR-0031's "additive, no behavior change for existing rows" claim: a JSON blob
    // shaped like a pre-ADR-0031 AnalysisConfig (no provider/model/endpoint/api_key fields)
    // still deserializes, defaulting to AzureFoundry.
    let json = serde_json::json!({
        "tenant_id": Uuid::new_v4(),
        "prompt": "flag policy violations",
        "updated_at": chrono::Utc::now(),
    });
    let config: AnalysisConfig = serde_json::from_value(json).unwrap();
    assert_eq!(config.provider, AnalysisProvider::AzureFoundry);
}

#[test]
fn provider_serializes_using_the_same_strings_the_postgres_repositories_store() {
    // Regression test: `rename_all = "snake_case"` would produce "open_ai_compatible" for
    // OpenAiCompatible ("Ai" is its own word), which doesn't match "openai_compatible" —
    // the string config-admin-service's and analysis-service's repositories store/parse by
    // hand. Wire format (this JSON) and storage format must use the same spelling.
    assert_eq!(
        serde_json::to_value(AnalysisProvider::AzureFoundry).unwrap(),
        serde_json::json!("azure_foundry")
    );
    assert_eq!(
        serde_json::to_value(AnalysisProvider::OpenAiCompatible).unwrap(),
        serde_json::json!("openai_compatible")
    );
}

#[test]
fn round_trips_through_json() {
    let config = AnalysisConfig::new(Uuid::new_v4(), "flag policy violations");
    let json = serde_json::to_string(&config).unwrap();
    let back: AnalysisConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, back);
}
