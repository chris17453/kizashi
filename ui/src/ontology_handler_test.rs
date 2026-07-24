use axum::response::IntoResponse;

#[test]
fn saved_ontology_view_round_trips_type_and_search_filters() {
    let query = common::SavedSearchQuery::new(
        uuid::Uuid::new_v4(),
        "At-risk customers",
        serde_json::json!({"surface": "ontology", "type_id": uuid::Uuid::nil(), "q": "contoso"}),
    );
    let view = super::to_saved_ontology_view(query);
    assert_eq!(view.name, "At-risk customers");
    assert!(view.load_url.contains("type_id="));
    assert!(view.load_url.contains("q=contoso"));
}

#[test]
fn saved_ontology_view_preserves_property_filters() {
    let query = common::SavedSearchQuery::new(
        uuid::Uuid::new_v4(),
        "Open cases",
        serde_json::json!({"surface": "ontology", "property": "status", "value": "open"}),
    );
    let view = super::to_saved_ontology_view(query);
    assert!(view.load_url.contains("property=status"));
    assert!(view.load_url.contains("value=open"));
}

#[test]
fn saved_ontology_view_preserves_relationship_scope() {
    let link_type_id = uuid::Uuid::new_v4();
    let query = common::SavedSearchQuery::new(
        uuid::Uuid::new_v4(),
        "Raised by links",
        serde_json::json!({"surface": "ontology", "link_type_id": link_type_id}),
    );
    let view = super::to_saved_ontology_view(query);
    assert!(view.load_url.contains(&format!("link_type_id={link_type_id}")));
}

#[test]
fn csv_escape_quotes_structured_values() {
    assert_eq!(super::csv_escape("status,open"), "\"status,open\"");
    assert_eq!(super::csv_escape("a\"b"), "\"a\"\"b\"");
}

#[test]
fn action_redirect_preserves_an_action_review_scope() {
    let response = super::action_redirect(
        Some("/actions?q=needs-review&outcome=review&from=2026-07-15"),
        "executed",
    )
    .into_response();
    assert_eq!(
        response.headers().get("location").unwrap(),
        "/actions?q=needs-review&outcome=review&from=2026-07-15&notice=executed"
    );
}

#[test]
fn object_context_joins_lineage_to_signal_and_case() {
    let record_id = uuid::Uuid::new_v4();
    let event_id = uuid::Uuid::new_v4();
    let incident_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let events = vec![crate::EventSummary {
        id: event_id,
        event_type: "risk.signal".to_string(),
        group_key: "customer-42".to_string(),
        status: "new".to_string(),
        occurred_at: now,
        record_ids: vec![record_id],
    }];
    let incidents = vec![crate::IncidentDetail {
        incident: common::Incident {
            id: incident_id,
            tenant_id: uuid::Uuid::new_v4(),
            title: "Customer risk review".to_string(),
            summary: "linked signal".to_string(),
            severity: common::IncidentSeverity::High,
            status: common::IncidentStatus::Open,
            assigned_to: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
        },
        event_ids: vec![event_id],
        notes: vec![],
    }];
    let (signals, cases) = super::object_operational_context(&[record_id], &events, &incidents);
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].id, event_id);
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].id, incident_id);
}

#[test]
fn ontology_graph_exposes_neighbor_isolation_control() {
    let template = include_str!("../templates/ontology.html");
    assert!(template.contains("data-graph-action=\"neighbors\""));
    assert!(template.contains("Show all entities"));
    assert!(template.contains("neighborMode"));
    assert!(template.contains("aria-pressed"));
    assert!(template.contains("updateGraphEdges"));
    assert!(template.contains("Drag nodes"));
    assert!(template.contains("localStorage.setItem(layoutKey"));
    assert!(template.contains("localStorage.removeItem(layoutKey"));
    assert_eq!(template.matches("class=\"pagination ontology-pagination\"").count(), 1);
    assert!(template.contains("&amp;risk={{ risk }}#object-{{ node.id }}"));
    assert!(template.contains("graph-relation-filter"));
    assert!(template.contains("matchesRelation"));
    assert!(template.contains("graphPrefsKey"));
    assert!(template.contains("persistGraphPrefs"));
    assert!(template.contains("data-graph-action=\"export\""));
    assert!(template.contains("kizashi-ontology-graph.svg"));
}

#[test]
fn ontology_object_activity_exposes_review_posture() {
    let template = include_str!("../templates/ontology.html");
    assert!(template.contains("Decision review posture"));
    assert!(template.contains("activity.review_status"));
    assert!(template.contains("activity.review_stale"));
    assert!(template.contains("/actions/{{ activity.id }}"));
}

#[test]
fn ontology_graph_expands_a_bounded_multi_hop_neighborhood() {
    let now = chrono::Utc::now();
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let third = uuid::Uuid::new_v4();
    let type_id = uuid::Uuid::new_v4();
    let objects = [first, second, third]
        .into_iter()
        .map(|id| common::ontology::Object {
            id,
            tenant_id: uuid::Uuid::new_v4(),
            object_type_id: type_id,
            properties: serde_json::json!({"id": id}),
            source_lineage: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        })
        .collect::<Vec<_>>();
    let links = [(first, second), (second, third)]
        .into_iter()
        .map(|(source_object_id, target_object_id)| common::ontology::Link {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            link_type_id: uuid::Uuid::new_v4(),
            source_object_id,
            target_object_id,
            properties: None,
            created_at: now,
            updated_at: now,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        super::graph_neighborhood(&objects, &links, Some(first), 1, 24),
        vec![first, second]
    );
    assert_eq!(
        super::graph_neighborhood(&objects, &links, Some(first), 2, 24),
        vec![first, second, third]
    );
}

#[test]
fn ontology_graph_finds_a_shortest_relationship_path() {
    let now = chrono::Utc::now();
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let third = uuid::Uuid::new_v4();
    let type_id = uuid::Uuid::new_v4();
    let objects = [first, second, third]
        .into_iter()
        .map(|id| common::ontology::Object {
            id,
            tenant_id: uuid::Uuid::new_v4(),
            object_type_id: type_id,
            properties: serde_json::json!({"id": id}),
            source_lineage: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        })
        .collect::<Vec<_>>();
    let links = [(first, second), (second, third)]
        .into_iter()
        .map(|(source_object_id, target_object_id)| common::ontology::Link {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            link_type_id: uuid::Uuid::new_v4(),
            source_object_id,
            target_object_id,
            properties: None,
            created_at: now,
            updated_at: now,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        super::shortest_object_path(&objects, &links, Some(first), Some(third)),
        vec![first, second, third]
    );
    assert!(super::shortest_object_path(&objects, &links, Some(first), Some(uuid::Uuid::new_v4()))
        .is_empty());
}

#[test]
fn ontology_graph_exposes_path_explorer_controls() {
    let template = include_str!("../templates/ontology.html");
    assert!(template.contains("Find shortest path"));
    assert!(template.contains("name=\"path_from\""));
    assert!(template.contains("ontology-path-result"));
    assert!(template.contains("No relationship path was found"));
}

#[test]
fn ontology_selection_exposes_side_by_side_comparison() {
    let template = include_str!("../templates/ontology.html");
    let compare = include_str!("../templates/ontology_compare.html");
    assert!(template.contains("id=\"ontology-compare-selected\""));
    assert!(template.contains("/ontology/compare?ids="));
    assert!(compare.contains("Property comparison"));
    assert!(compare.contains("missing properties are shown as an em dash"));
}

#[test]
fn ontology_compare_query_accepts_a_bounded_id_set() {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let query: super::OntologyCompareQuery =
        serde_urlencoded::from_str(&format!("ids={first},{second}")).unwrap();
    let ids = query
        .ids
        .split(',')
        .filter_map(|value| uuid::Uuid::parse_str(value).ok())
        .take(6)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![first, second]);
}

#[test]
fn ontology_relationship_filter_is_exposed_as_a_live_scope() {
    let template = include_str!("../templates/ontology.html");
    assert!(template.contains("name=\"link_type_id\""));
    assert!(template.contains("{{ matching_link_count }} live instance"));
}

#[test]
fn ontology_property_coverage_measures_declared_field_population() {
    let now = chrono::Utc::now();
    let type_id = uuid::Uuid::new_v4();
    let object_type = common::ontology::ObjectType {
        id: type_id,
        tenant_id: uuid::Uuid::new_v4(),
        name: "Customer".to_string(),
        version: 1,
        property_schema: serde_json::json!({"name": {"type": "string"}, "status": {"type": "string"}}),
        mapping_rules: serde_json::json!([]),
        created_at: now,
        updated_at: now,
    };
    let objects = vec![
        common::ontology::Object {
            id: uuid::Uuid::new_v4(),
            tenant_id: object_type.tenant_id,
            object_type_id: type_id,
            properties: serde_json::json!({"name": "Ada", "status": "active"}),
            source_lineage: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        },
        common::ontology::Object {
            id: uuid::Uuid::new_v4(),
            tenant_id: object_type.tenant_id,
            object_type_id: type_id,
            properties: serde_json::json!({"name": "Grace", "status": ""}),
            source_lineage: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        },
    ];
    let (fields, rows) = super::property_coverage(&[object_type], &objects);
    assert_eq!(fields, vec!["name", "status"]);
    assert_eq!(rows[0].object_count, 2);
    assert_eq!(rows[0].cells[0].percent, 100);
    assert_eq!(rows[0].cells[1].percent, 50);
    assert_eq!(rows[0].cells[0].tone, "good");
    assert_eq!(rows[0].cells[1].tone, "danger");
}

#[test]
fn ontology_risk_scope_survives_object_navigation_controls() {
    let template = include_str!("../templates/ontology.html");
    assert!(template.contains("name=\"risk\" value=\"{{ risk }}\""));
    assert!(template.contains("&risk={{ risk|urlencode }}&q="));
    assert!(template.contains("ontology-save-view-form"));
}

#[test]
fn ontology_view_save_preserves_the_active_scope() {
    let form = super::SaveOntologyViewForm {
        name: "At-risk customers".to_string(),
        type_id: Some(uuid::Uuid::nil()),
        q: "Northwind".to_string(),
        property: "status".to_string(),
        value: "at-risk".to_string(),
        risk: "critical".to_string(),
        link_type_id: Some(uuid::Uuid::from_u128(30)),
    };
    let response = super::ontology_view_redirect(&form, "view_saved").into_response();
    let location = response.headers().get("location").unwrap().to_str().unwrap().to_string();
    assert!(location.contains("type_id=00000000-0000-0000-0000-000000000000"));
    assert!(location.contains("q=Northwind"));
    assert!(location.contains("property=status"));
    assert!(location.contains("risk=critical"));
    assert!(location.contains("link_type_id=00000000-0000-0000-0000-00000000001e"));
}
