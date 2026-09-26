use super::*;

#[test]
fn build_studio_template_names_both_configuration_planes() {
    let body = BuildStudioPage {
        show_nav: true,
        is_admin: true,
        sources: vec!["ERP".to_string()],
        pipelines: vec!["Invoices".to_string()],
        error: None,
    }
    .render()
    .unwrap();
    assert!(body.contains("Data Sources") && body.contains("Pipelines"));
}

#[test]
fn data_source_form_rejects_persisted_credentials() {
    let result = source_from_form(
        DataSourceForm {
            name: "ERP".to_string(),
            kind: "api".to_string(),
            mode: "read".to_string(),
            connection: r#"{"token":"do-not-store"}"#.to_string(),
            ..Default::default()
        },
        Uuid::new_v4(),
    );
    assert!(result.is_err());
}

#[test]
fn data_source_template_has_list_and_versioned_detail_paths() {
    assert!(include_str!("../templates/data_sources.html")
        .contains("/build/data-sources/{{ source.id }}"));
    assert!(include_str!("../templates/data_source_detail.html")
        .contains("Save version {{ source.version }}"));
}
