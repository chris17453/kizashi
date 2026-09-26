use super::*;

#[test]
fn action_template_options_serialize_only_the_selector_contract() {
    let id = Uuid::new_v4();
    let json = action_templates_json(&[ActionTemplateOption {
        id,
        name: "Escalate case".to_string(),
        action_type: "webhook".to_string(),
    }]);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value[0]["id"], id.to_string());
    assert_eq!(value[0]["name"], "Escalate case");
    assert_eq!(value[0]["action_type"], "webhook");
}

#[test]
fn action_template_options_escape_html_script_delimiters() {
    let json = action_templates_json(&[ActionTemplateOption {
        id: Uuid::new_v4(),
        name: "</script><script>alert(1)</script>".to_string(),
        action_type: "custom".to_string(),
    }]);
    assert!(!json.contains("</script>"));
    assert!(json.contains("\\u003c/script\\u003e"));
}

#[test]
fn edit_form_accepts_an_optional_action_template_id() {
    let id = Uuid::new_v4();
    let form: EditTriggerForm = serde_urlencoded::from_str(&format!(
        "name=rule&event_type_match=risk&window_seconds=60&condition=%7B%7D&actions=%5B%5D&action_template_id={id}"
    ))
    .unwrap();
    assert_eq!(form.action_template_id, Some(id));
}
