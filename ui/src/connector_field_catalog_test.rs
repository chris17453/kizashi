use super::*;

#[test]
fn every_connector_type_has_a_display_name() {
    for (connector_type, _) in CONNECTOR_TYPES {
        assert!(display_name(connector_type).is_some());
    }
}

#[test]
fn unknown_connector_type_has_no_display_name() {
    assert_eq!(display_name("nonexistent"), None);
}

#[test]
fn every_connector_type_has_at_least_one_field() {
    for (connector_type, _) in CONNECTOR_TYPES {
        assert!(!fields_for(connector_type).is_empty(), "{connector_type} has no fields");
    }
}

#[test]
fn unknown_connector_type_has_no_fields() {
    assert!(fields_for("nonexistent").is_empty());
}

#[test]
fn zendesk_requires_subdomain_email_and_token() {
    let fields = fields_for("zendesk");
    let env_vars: Vec<&str> = fields.iter().map(|f| f.env_var).collect();
    assert!(env_vars.contains(&"ZENDESK_SUBDOMAIN"));
    assert!(env_vars.contains(&"ZENDESK_EMAIL"));
    assert!(env_vars.contains(&"ZENDESK_API_TOKEN"));
}

#[test]
fn secret_fields_are_marked_secret() {
    let fields = fields_for("zendesk");
    let token_field = fields.iter().find(|f| f.env_var == "ZENDESK_API_TOKEN").unwrap();
    assert!(token_field.secret);
    let subdomain_field = fields.iter().find(|f| f.env_var == "ZENDESK_SUBDOMAIN").unwrap();
    assert!(!subdomain_field.secret);
}
