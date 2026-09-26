use super::OntologyObject360Template;
use askama::Template;

#[test]
fn object_workspace_renders_the_versioned_read_model_route() {
    let id = uuid::Uuid::new_v4();
    let body = OntologyObject360Template {
        show_nav: true,
        is_admin: true,
        can_manage: true,
        object_id: id,
    }
    .render()
    .expect("object workspace should render");
    assert!(body.contains("/api/v1/ontology/objects/"));
    assert!(body.contains(&format!("const id = '{id}'")));
    assert!(body.contains("Related objects"));
    assert!(body.contains("Relationship graph"));
    assert!(body.contains("Source evidence"));
    assert!(body.contains("Export investigation"));
    assert!(body.contains("exportInvestigation"));
    assert!(body.contains("cache: 'no-store'"));
    assert!(body.contains("Object 360 export unavailable"));
    assert!(body.contains("application/json"));
    assert!(body.contains("object360-source-records"));
    assert!(body.contains("source_records"));
    assert!(body.contains("object360-graph"));
    assert!(body.contains("history_href"));
    assert!(body.contains("Open immutable relationship history"));
    assert!(body.contains("Inspect immutable edge history"));
    assert!(body.contains("object360-edge-history"));
    assert!(body.contains("Edit governed relationship instance"));
    assert!(body.contains("/ontology/links/instances/"));
    assert!(body.contains("properties_schema"));
    assert!(body.contains("Create relationship"));
    assert!(body.contains("object360-relationship-options"));
    assert!(body.contains("relationship_options"));
    assert!(body.contains("/ontology/links/instances"));
    assert!(body.contains("eligible"));
    assert!(body.contains("cardinality"));
    assert!(body.contains("applyRelationshipEligibility"));
    assert!(body.contains("Refresh context"));
    assert!(body.contains("refreshObject360"));
    assert!(body.contains("object360-refresh-status"));
    assert!(!body.contains("MutationObserver"));
    assert!(body.contains("details.appendChild(deleteForm)"));
    assert!(body.contains("Delete relationship"));
    assert!(body.contains("return_to"));
    assert!(body.contains("renderGraph"));
    assert!(body.contains("Open full graph"));
    assert!(body.contains("Investigation timeline"));
    assert!(body.contains("object360-timeline"));
    assert!(body.contains("object360-timeline-kind"));
    assert!(body.contains("object360-timeline-query"));
    assert!(body.contains("applyTimelineFilters"));
    assert!(body.contains("object360-timeline-from"));
    assert!(body.contains("object360-timeline-to"));
    assert!(body.contains("Timeline start date"));
    assert!(body.contains("entries shown"));
    assert!(body.contains("Decision history"));
    assert!(body.contains("Update modeled state"));
    assert!(body.contains("Preserve investigation focus"));
    assert!(body.contains("Investigation annotations"));
    assert!(body.contains("object360-annotations"));
    assert!(body.contains("object360-annotation-count"));
    assert!(body.contains("/ontology/objects/") && body.contains("/annotations"));
    assert!(body.contains("data.annotations"));
    assert!(body.contains("/ontology/saved-views"));
    assert!(body.contains("name=\"object_ids\""));
    assert!(body.contains("data-object360-property-editor"));
    assert!(body.contains("schema-aware fields"));
    assert!(body.contains("object360PropertyKey"));
    assert!(body.contains("/ontology/objects/") && body.contains("/edit"));
    assert!(body.contains("data-object360-properties"));
    assert!(body.contains("Available governed actions"));
    assert!(body.contains("object360-available-actions"));
    assert!(body.contains("dataset.object360ActionField"));
    assert!(body.contains("parameter_schema"));
    assert!(body.contains("canonical = '/ontology/objects/'"));
    let handler = include_str!("ontology_handler.rs");
    assert!(handler.contains("pub return_to: String"));
    assert!(handler.contains("let object_route = format!(\"/ontology/objects/{id}/360\")"));
    assert!(handler.contains("pub struct OntologyReturnQuery"));
    assert!(handler.contains("relationship_deleted"));
}
