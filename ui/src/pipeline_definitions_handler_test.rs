use super::*;

#[test]
fn command_pipeline_requires_a_write_back_step() {
    let result = pipeline_from_form(
        PipelineDefinitionForm {
            name: "Update ERP".to_string(),
            data_source_id: Uuid::new_v4().to_string(),
            mode: "command".to_string(),
            steps: r#"[{"kind":"extract","config":{}}]"#.to_string(),
            ..Default::default()
        },
        Uuid::new_v4(),
    );
    assert!(result.is_err());
}

#[test]
fn pipeline_templates_link_list_and_detail_routes() {
    assert!(include_str!("../templates/pipeline_definitions.html")
        .contains("/build/pipelines/{{ pipeline.id }}"));
    assert!(include_str!("../templates/pipeline_definition_detail.html")
        .contains("Save version {{ pipeline.version }}"));
}
