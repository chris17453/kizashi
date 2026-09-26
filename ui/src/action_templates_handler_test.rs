use super::*;

#[test]
fn parses_every_trigger_provider_type() {
    for value in ["email", "webhook", "teams_alert", "create_ticket", "custom"] {
        assert!(parse_action_type(value).is_some());
    }
}

#[test]
fn template_form_requires_object_config() {
    let result = template_from_form(
        ActionTemplateForm {
            name: "Template".to_string(),
            action_type: "webhook".to_string(),
            config: "[]".to_string(),
            ..Default::default()
        },
        Uuid::new_v4(),
    );
    assert!(result.is_err());
}

#[test]
fn action_template_page_exposes_versioned_edit_control() {
    let template = include_str!("../templates/action_templates.html");
    assert!(template.contains("/action-templates/{{ template.id }}/edit"));
    assert!(template.contains("Save new version"));
}
