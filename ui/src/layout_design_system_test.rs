#[test]
fn query_bars_use_explicit_field_groups_instead_of_the_legacy_inline_form() {
    for template in [
        include_str!("../templates/recent_audit_log.html"),
        include_str!("../templates/incidents.html"),
        include_str!("../templates/actions.html"),
    ] {
        assert!(template.contains("class=\"query-bar"));
        assert!(template.contains("class=\"query-field"));
        assert!(template.contains("class=\"query-actions\""));
    }
}

#[test]
fn shared_layout_defines_a_responsive_query_bar_contract() {
    let layout = include_str!("../templates/layout.html");

    assert!(layout.contains(".query-bar {"));
    assert!(layout.contains(".query-field--wide"));
    assert!(layout.contains(".query-actions"));
    assert!(layout.contains(".query-bar { grid-template-columns: repeat(2, minmax(0, 1fr)); }"));
}
