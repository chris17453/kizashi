#[test]
fn action_detail_exposes_fresh_bounded_investigation_export() {
    let template = include_str!("../templates/action_detail.html");
    assert!(template.contains("action-360-export"));
    assert!(template.contains("/api/v1/actions/"));
    assert!(template.contains("cache: 'no-store'"));
    assert!(template.contains("decision-investigation-"));
    assert!(template.contains("Decision investigation unavailable"));
}
