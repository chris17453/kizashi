use super::*;
use crate::health_client::ServiceHealthSummary;

fn health_with(services: Vec<(&str, &str)>) -> PlatformHealthSummary {
    PlatformHealthSummary {
        status: "up".to_string(),
        services: services
            .into_iter()
            .map(|(name, status)| ServiceHealthSummary {
                name: name.to_string(),
                status: status.to_string(),
            })
            .collect(),
    }
}

#[test]
fn builds_five_stages_interleaved_with_four_edges() {
    let health = health_with(vec![]);
    let items = build_topology_items(&health, &[]);

    let stages = items.iter().filter(|i| matches!(i, TopologyItem::Stage { .. })).count();
    let edges = items.iter().filter(|i| matches!(i, TopologyItem::Edge { .. })).count();
    assert_eq!(stages, 5);
    assert_eq!(edges, 4);
    assert!(matches!(items[0], TopologyItem::Stage { .. }));
    assert!(matches!(items[1], TopologyItem::Edge { .. }));
}

#[test]
fn a_stage_missing_from_health_is_reported_unknown_not_down() {
    let health = health_with(vec![]);
    let items = build_topology_items(&health, &[]);

    let TopologyItem::Stage { status, .. } = &items[0] else { panic!("expected a stage") };
    assert_eq!(status, "unknown");
}

#[test]
fn a_stage_present_in_health_reports_its_real_status() {
    let health = health_with(vec![("ingestion-service", "down")]);
    let items = build_topology_items(&health, &[]);

    let TopologyItem::Stage { status, .. } = &items[0] else { panic!("expected a stage") };
    assert_eq!(status, "down");
}

#[test]
fn severity_thresholds_are_ok_warn_critical() {
    assert_eq!(severity_for(0), "ok");
    assert_eq!(severity_for(1), "warn");
    assert_eq!(severity_for(CRITICAL_BACKLOG_THRESHOLD - 1), "warn");
    assert_eq!(severity_for(CRITICAL_BACKLOG_THRESHOLD), "critical");
}

#[test]
fn an_edge_with_no_backlog_data_is_marked_unknown() {
    let health = health_with(vec![]);
    let items = build_topology_items(&health, &[]);

    let TopologyItem::Edge { messages, severity, .. } = &items[1] else {
        panic!("expected an edge")
    };
    assert_eq!(*messages, None);
    assert_eq!(*severity, "unknown");
}

#[test]
fn an_edge_with_real_backlog_data_reports_its_message_count() {
    let health = health_with(vec![]);
    let depths = vec![QueueDepthSummary {
        stage: "ingest_to_normalize".to_string(),
        queue_name: "normalization-service.record.ingested".to_string(),
        messages: 500,
    }];
    let items = build_topology_items(&health, &depths);

    let TopologyItem::Edge { messages, severity, .. } = &items[1] else {
        panic!("expected an edge")
    };
    assert_eq!(*messages, Some(500));
    assert_eq!(*severity, "critical");
}
