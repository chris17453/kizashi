#[test]
fn apps_template_composes_every_stable_surface() {
    let template = include_str!("../templates/apps.html");
    for surface in
        ["Generated forms and tables", "Uploads", "Exception queues", "Dashboards", "Detail views"]
    {
        assert!(template.contains(surface));
    }
}

#[test]
fn app_detail_template_has_versioned_edit_controls() {
    let template = include_str!("../templates/app_detail.html");
    assert!(template.contains("Save version {{ app.version }}"));
}
