use super::workspace_login_target;

#[test]
fn workspace_switch_preserves_a_prefilled_tenant_name() {
    assert_eq!(workspace_login_target("acme"), "/login?tenant_name=acme");
    assert_eq!(workspace_login_target("acme north"), "/login?tenant_name=acme+north");
}

#[test]
fn workspace_switch_defaults_to_the_plain_login_page_without_a_tenant() {
    assert_eq!(workspace_login_target("  "), "/login");
}
