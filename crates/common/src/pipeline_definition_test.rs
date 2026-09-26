use super::*;
use serde_json::json;
use uuid::Uuid;

fn pipeline() -> PipelineDefinition {
    PipelineDefinition {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "ERP inventory projection".to_string(),
        description: "Projects selected external inventory fields".to_string(),
        data_source_id: Uuid::new_v4(),
        target_object_type_id: Some(Uuid::new_v4()),
        mode: PipelineMode::Projection,
        steps: json!([
            {"kind": "extract", "config": {"table": "inventory"}},
            {"kind": "transform", "config": {"operation": "select_fields", "fields": ["sku", "quantity"]}},
            {"kind": "validate", "config": {"contract": "Inventory"}},
            {"kind": "match", "config": {"key": "sku"}},
            {"kind": "route", "config": {"target": "ontology"}}
        ]),
        enabled: true,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn projection_pipeline_accepts_declarative_steps() {
    assert!(validate_pipeline_definition(&pipeline()).is_ok());
}

#[test]
fn pipeline_contract_rejects_unsafe_or_incomplete_write_back_definitions() {
    let mut unsafe_transform = pipeline();
    unsafe_transform.steps[1]["config"]["script"] = json!("fetch('https://untrusted')");
    assert!(validate_pipeline_definition(&unsafe_transform).is_err());

    let mut projection_write_back = pipeline();
    projection_write_back
        .steps
        .as_array_mut()
        .unwrap()
        .push(json!({"kind": "write_back", "config": {}}));
    assert!(validate_pipeline_definition(&projection_write_back).is_err());

    let mut missing_write_back = pipeline();
    missing_write_back.mode = PipelineMode::Command;
    assert!(validate_pipeline_definition(&missing_write_back).is_err());
}
