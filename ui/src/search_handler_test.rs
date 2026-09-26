#[test]
fn search_results_expose_typed_entity_selection_handoff() {
    let template = include_str!("../templates/search.html");
    assert!(template.contains("data-search-entity-id"));
    assert!(template.contains("data-search-entity-type"));
    assert!(template.contains("data-search-compare"));
    assert!(template.contains("data-search-load"));
    assert!(template.contains("kizashi.ontology.selection-types"));
    assert!(template.contains("/ontology/compare?ids="));
}

#[test]
fn entity_search_hits_retain_object_type_identity() {
    let source = include_str!("search_handler.rs");
    assert!(source.contains("object_type_id: uuid::Uuid"));
    assert!(source.contains("object_type_id: object.object_type_id"));
}
