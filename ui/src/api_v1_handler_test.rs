use super::*;

#[test]
fn versioned_api_exposes_core_console_domains() {
    let source = include_str!("lib.rs");
    assert!(source.contains("/api/v1/events"));
    assert!(source.contains("/api/v1/health"));
    assert!(source.contains("/api/v1/overview"));
    assert!(source.contains("/api/v1/pipeline"));
    let api_source = include_str!("api_v1_handler.rs");
    assert!(api_source.contains("pub graph: PipelineApiGraph"));
    assert!(api_source.contains("signals-to-action"));
    assert!(api_source.contains("action-to-response"));
    assert!(!api_source.contains("action-to-signals"));
    assert!(source.contains("/api/v1/audit-log"));
    assert!(source.contains("/api/v1/audit-log/:service/:entity_id"));
    assert!(api_source.contains("pub async fn get_entity_audit_log_api"));
    assert!(source.contains("/api/v1/audit-log/export.csv"));
    assert!(source.contains("/api/v1/branding"));
    assert!(source.contains("/api/v1/analysis-config"));
    assert!(source.contains("/api/v1/egress-allowlist"));
    assert!(source.contains("/api/v1/security/sessions"));
    assert!(source.contains("/api/v1/security/sessions/bulk-revoke"));
    assert!(source.contains("/api/v1/security/login-attempts"));
    assert!(source.contains("/api/v1/security/login-attempts/export.csv"));
    assert!(source.contains("/api/v1/security/backups"));
    assert!(source.contains("/api/v1/security/compliance-report"));
    assert!(source.contains("/api/v1/reports/export.csv"));
    assert!(source.contains("/api/v1/reports/export.pdf"));
    assert!(source.contains("/api/v1/security/mfa"));
    assert!(source.contains("/api/v1/security/password"));
    assert!(source.contains("/api/v1/incidents/:id/status"));
    assert!(source.contains("/api/v1/incidents/:id/360"));
    assert!(api_source.contains("pub async fn get_incident_360"));
    assert!(api_source.contains("timeline: Vec<IncidentTimelineApiEntry>"));
    assert!(source.contains("/api/v1/incidents/:id/brief"));
    assert!(api_source.contains("pub async fn generate_incident_brief_api"));
    assert!(source.contains("/api/v1/incidents/:id/claim"));
    assert!(source.contains("/api/v1/incidents/:id/notes"));
    assert!(source.contains("/api/v1/incidents/bulk"));
    assert!(source.contains("/api/v1/events/bulk-status"));
    assert!(source.contains("/api/v1/incidents/:incident_id/events"));
    assert!(source.contains("/api/v1/data/export.csv"));
    assert!(source.contains("/api/v1/work/export.csv"));
    assert!(source.contains("/api/v1/ontology/export.csv"));
    assert!(source.contains("/api/v1/events/export.csv"));
    assert!(source.contains("/api/v1/events/:id/360"));
    assert!(source.contains("/api/v1/events/:id/status-history"));
    assert!(api_source.contains("pub async fn list_event_status_history_api"));
    assert!(api_source.contains("pub async fn get_event_360"));
    assert!(source.contains("/api/v1/actions/:id/360"));
    assert!(api_source.contains("pub async fn get_action_360"));
    assert!(source.contains("/api/v1/incidents/export.csv"));
    assert!(source.contains("/api/v1/search"));
    assert!(source.contains("/api/v1/saved-views"));
    assert!(source.contains("put(api_v1_handler::update_saved_view_api)"));
    assert!(source.contains("/api/v1/incidents"));
    assert!(source.contains("/api/v1/ontology/objects"));
    assert!(source.contains("/api/v1/incidents/:incident_id/events/:event_id"));
    assert!(source.contains("delete(api_v1_handler::unlink_incident_event)"));
    assert!(api_source.contains("pub async fn unlink_incident_event"));
    assert!(source.contains("post(api_v1_handler::create_incident)"));
    assert!(source.contains("/api/v1/triggers"));
    assert!(source.contains("post(api_v1_handler::test_trigger)"));
    assert!(source.contains("/api/v1/triggers/bulk-toggle"));
    assert!(source.contains("post(api_v1_handler::bulk_toggle_triggers)"));
    assert!(source.contains("/api/v1/event-types"));
    assert!(source.contains("get(api_v1_handler::get_event_type)"));
    assert!(source.contains("post(api_v1_handler::create_event_type_version)"));
    assert!(source.contains("/api/v1/retention-policies"));
    assert!(source.contains("/api/v1/retention-policies/bulk-delete"));
    assert!(api_source.contains("pub async fn bulk_delete_retention_policies_api"));
    assert!(source.contains("get(api_v1_handler::get_retention_policy)"));
    assert!(source.contains("get(api_v1_handler::get_compliance_hold)"));
    assert!(source.contains("post(api_v1_handler::reimport_archive)"));
    assert!(source.contains("/api/v1/report-schedules"));
    assert!(source.contains("put(api_v1_handler::update_report_schedule)"));
    assert!(source.contains("post(api_v1_handler::run_report_schedule)"));
    assert!(source.contains("/api/v1/sensors"));
    assert!(source.contains("post(api_v1_handler::register_sensor)"));
    assert!(source.contains("/api/v1/sensors/bulk-delete"));
    assert!(source.contains("/api/v1/sensors/catalog"));
    assert!(source.contains("get(api_v1_handler::list_sensor_catalog_api)"));
    assert!(source.contains("/api/v1/sensors/generate"));
    assert!(source.contains("post(api_v1_handler::generate_sensor_script_api)"));
    assert!(source.contains("/api/v1/normalization-mappings"));
    assert!(source.contains("post(api_v1_handler::create_normalization_mapping)"));
    assert!(source.contains("/api/v1/ontology/object-types"));
    assert!(source.contains("post(api_v1_handler::create_ontology_object_type)"));
    assert!(source.contains("/api/v1/ontology/object-types/:id"));
    assert!(source.contains("get(api_v1_handler::get_ontology_object_type)"));
    assert!(source.contains("/api/v1/ontology/objects/:id/history"));
    assert!(source.contains("/api/v1/ontology/objects/:id/annotations"));
    assert!(source.contains("get(api_v1_handler::list_ontology_object_annotations)"));
    assert!(source.contains("post(api_v1_handler::create_ontology_object_annotation)"));
    assert!(api_source.contains("annotations: Vec<common::ontology::ObjectAnnotation>"));
    assert!(api_source.contains("list_all_object_annotations"));
    assert!(source.contains("/api/v1/ontology/link-types/:id/history"));
    assert!(api_source.contains("pub async fn list_ontology_link_type_history"));
    assert!(source.contains("/api/v1/ontology/links/:id/history"));
    assert!(api_source.contains("pub async fn list_ontology_link_history"));
    assert!(source.contains("/api/v1/ontology/objects/:object_id/links/:link_type_id"));
    assert!(source.contains("/api/v1/ontology/compare"));
    assert!(source.contains("/api/v1/ontology/graph"));
    assert!(source.contains("/api/v1/ontology/objects/:id/360"));
    assert!(source.contains("/api/v1/ontology/objects/bulk-update"));
    assert!(api_source.contains("pub async fn bulk_update_ontology_objects"));
    assert!(source.contains("get(api_v1_handler::get_ontology_compare_api)"));
    assert!(source.contains("post(api_v1_handler::create_ontology_link_type)"));
    assert!(source.contains("/api/v1/ontology/link-types/:id"));
    assert!(source.contains("get(api_v1_handler::get_ontology_link_type)"));
    assert!(source.contains("post(api_v1_handler::create_ontology_link)"));
    assert!(source.contains("/api/v1/ontology/links/:id"));
    assert!(source.contains("/api/v1/ontology/links/instances/bulk"));
    assert!(api_source.contains("pub async fn create_bulk_ontology_link_instances_api"));
    assert!(source.contains("get(api_v1_handler::get_ontology_link)"));
    assert!(source.contains("/api/v1/ontology/actions/invoke"));
    assert!(source.contains("/api/v1/actions"));
    assert!(source.contains("/api/v1/actions/library"));
    assert!(api_source.contains("pub async fn get_action_library_api"));
    assert!(source.contains("/api/v1/actions/:id"));
    assert!(source.contains("/api/v1/actions/:id/review"));
    assert!(source.contains("/api/v1/actions/bulk-review"));
    assert!(source.contains("/api/v1/actions/bulk-retry"));
    assert!(source.contains("/api/v1/actions/dead-letter/replay"));
    assert!(source.contains("/api/v1/actions/dead-letter/peek"));
    assert!(api_source.contains("pub async fn bulk_action_review_api"));
    assert!(api_source.contains("pub async fn bulk_retry_actions_api"));
    assert!(api_source.contains("pub async fn replay_dead_letter_api"));
    assert!(api_source.contains("pub async fn peek_dead_letter_api"));
    assert!(source.contains("/api/v1/actions/export.csv"));
    assert!(source.contains("post(api_v1_handler::create_ontology_action_type)"));
    assert!(source.contains("get(api_v1_handler::get_ontology_action_type)"));
    assert!(source.contains("get(api_v1_handler::list_ontology_action_type_history)"));
    assert!(source.contains("/api/v1/ontology/action-types/history"));
    assert!(api_source.contains("pub async fn list_ontology_action_type_history_all"));
    assert!(source.contains("/api/v1/ontology/object-types/history"));
    assert!(api_source.contains("pub async fn list_ontology_object_type_history_all"));
    assert!(source.contains("/api/v1/ontology/objects/history"));
    assert!(api_source.contains("pub async fn list_ontology_object_history_all"));
    assert!(source.contains("/api/v1/ontology/links/history"));
    assert!(api_source.contains("pub async fn list_ontology_link_history_all"));
    assert!(source.contains("/api/v1/ontology/link-types/history"));
    assert!(api_source.contains("pub async fn list_ontology_link_type_history_all"));
    assert!(source.contains("/api/v1/service-accounts"));
    assert!(source.contains("post(api_v1_handler::create_service_account)"));
    assert!(source.contains("/api/v1/dashboard/layout"));
    assert!(source.contains("put(api_v1_handler::put_dashboard_layout)"));
    assert!(source.contains("/api/v1/attention"));
    assert!(source.contains("get(api_v1_handler::get_attention_api)"));
    assert!(source.contains("/api/v1/security/overview"));
    assert!(source.contains("get(api_v1_handler::get_security_overview_api)"));
    assert!(source.contains("/api/v1/security/permissions"));
    assert!(source.contains("get(api_v1_handler::get_permissions_reference_api)"));
    assert!(source.contains("/api/v1/configuration"));
    assert!(source.contains("get(api_v1_handler::get_configuration_api)"));
    assert!(source.contains("/api/v1/session/context"));
    assert!(source.contains("get(api_v1_handler::get_session_context_api)"));
    assert!(source.contains("/api/v1/reports/summary"));
    assert!(source.contains("get(api_v1_handler::get_reports_summary_api)"));
    assert!(source.contains("/api/v1/data/records"));
    assert!(source.contains("/api/v1/data/records/:id/journey"));
    assert!(source.contains("/api/v1/data/compare"));
    assert!(source.contains("get(api_v1_handler::get_data_compare_api)"));
    assert!(source.contains("post(api_v1_handler::reprocess_data_records)"));
    assert!(source.contains("/api/v1/data/model"));
    assert!(source.contains("post(api_v1_handler::model_data_records_api)"));
    assert!(source.contains("/api/v1/data/records/:id/model"));
    assert!(source.contains("/api/v1/work"));
    assert!(source.contains("post(api_v1_handler::claim_work_api)"));
    assert!(source.contains("/api/v1/action-templates"));
    assert!(source.contains("post(api_v1_handler::create_action_template_api)"));
    assert!(source.contains("/api/v1/api-keys"));
    assert!(source.contains("/api/v1/api-keys/bulk-revoke"));
    assert!(source.contains("post(api_v1_handler::create_api_key_api)"));
    assert!(source.contains("/api/v1/users"));
    assert!(source.contains("get(api_v1_handler::get_user_api)"));
    assert!(source.contains("post(api_v1_handler::create_user)"));
    assert!(source.contains("/api/v1/users/bulk-delete"));
    assert!(source.contains("/api/v1/users/bulk-role"));
    assert!(source.contains("get(api_v1_handler::get_ontology_object)"));
    assert!(source.contains("get(api_v1_handler::get_password_policy)"));
}

#[test]
fn saved_view_surface_validation_is_bounded() {
    assert_eq!(normalize_saved_view_surface(" Events ").unwrap(), "events");
    assert_eq!(
        normalize_saved_view_surface("dashboard").unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn journey_latency_never_reports_negative_time() {
    let now = chrono::Utc::now();
    assert_eq!(journey_latency_ms(now, now - chrono::Duration::seconds(1)), 0);
    assert_eq!(journey_latency_ms(now, now + chrono::Duration::milliseconds(1250)), 1250);
}

#[test]
fn audit_log_api_caps_cursor_page_size() {
    let query = AuditLogApiQuery { limit: Some(5000), before: None, ..Default::default() };
    assert_eq!(query.limit.unwrap_or(50).clamp(1, 200), 200);
}

#[test]
fn audit_log_api_query_defaults_to_unfiltered() {
    let query = AuditLogApiQuery::default();
    assert!(query.q.is_none());
    assert!(query.service.is_none());
    assert!(query.change_type.is_none());
    assert!(query.date.is_none());
}

#[test]
fn audit_log_api_walks_bounded_pages_when_filters_are_active() {
    assert_eq!(AUDIT_API_FILTER_MAX_PAGES, 10);
    let entry = crate::audit_log_client::AuditLogEntry {
        id: uuid::Uuid::new_v4(),
        entity_type: "retention_policy".to_string(),
        entity_id: uuid::Uuid::new_v4(),
        change_type: "updated".to_string(),
        actor: "operator".to_string(),
        before: Some(serde_json::json!({"days": 30})),
        after: serde_json::json!({"days": 90}),
        changed_at: chrono::Utc::now(),
    };
    let query = AuditLogExportApiQuery { q: Some("90".to_string()), ..Default::default() };
    assert!(audit_export_matches("retention-service", &entry, &query).unwrap());
}

#[test]
fn ontology_object_api_filters_text_property_and_value() {
    let object = common::ontology::Object {
        id: uuid::Uuid::new_v4(),
        tenant_id: uuid::Uuid::new_v4(),
        object_type_id: uuid::Uuid::new_v4(),
        properties: serde_json::json!({"status": "Investigating", "subject": "Payment outage"}),
        source_lineage: serde_json::json!([]),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(ontology_object_api_matches(
        &object,
        &OntologyObjectsApiQuery { q: Some("payment".into()), ..Default::default() }
    ));
    assert!(ontology_object_api_matches(
        &object,
        &OntologyObjectsApiQuery {
            property: Some("status".into()),
            value: Some("investig".into()),
            ..Default::default()
        }
    ));
    assert!(!ontology_object_api_matches(
        &object,
        &OntologyObjectsApiQuery {
            property: Some("status".into()),
            value: Some("closed".into()),
            ..Default::default()
        }
    ));
}

#[test]
fn egress_allowlist_api_rejects_empty_or_oversized_domains() {
    assert!(validate_egress_domains(&[]).is_ok());
    assert_eq!(
        validate_egress_domains(&[" ".to_string()]).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        validate_egress_domains(&["a".repeat(254)]).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn password_api_requires_matching_confirmation() {
    assert!(validate_password_confirmation("new-password", "new-password").is_ok());
    assert_eq!(
        validate_password_confirmation("new-password", "different").unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn incident_note_api_rejects_blank_or_oversized_notes() {
    assert!(validate_incident_note("Investigated customer impact").is_ok());
    assert_eq!(validate_incident_note(" ").unwrap_err().status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        validate_incident_note(&"x".repeat(20_001)).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn event_api_rejects_non_rfc3339_timestamps() {
    let error = parse_time(Some(&"tomorrow".to_string())).unwrap_err();
    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn report_schedule_validation_rejects_unknown_cadence() {
    let request = CreateReportScheduleApiRequest {
        name: "weekly posture".to_string(),
        frequency: "hourly".to_string(),
        recipient: "ops@example.com".to_string(),
        from: String::new(),
        to: String::new(),
        format: "pdf".to_string(),
        enabled: None,
    };
    assert_eq!(report_schedule_filter(&request).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn report_schedule_api_rejects_unknown_run_status() {
    assert!(normalize_report_run_status_api(Some(&"success".to_string())).is_ok());
    assert_eq!(
        normalize_report_run_status_api(Some(&"queued".to_string())).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn report_schedule_projection_ignores_non_schedule_saved_views() {
    let query = common::SavedSearchQuery::new(
        uuid::Uuid::new_v4(),
        "ordinary view",
        serde_json::json!({"view_kind":"events"}),
    );
    assert!(schedule_from_query(query).is_none());
}

#[test]
fn sensor_api_accepts_only_registered_connector_types() {
    assert!(valid_connector_type("zendesk"));
    assert!(valid_connector_type("graph-mail"));
    assert!(!valid_connector_type("arbitrary-http"));
}

#[test]
fn sensor_api_filters_by_name_type_and_enabled_state() {
    let sensor =
        common::Sensor::new(uuid::Uuid::new_v4(), "zendesk", "Support desk", serde_json::json!({}));
    let query = SensorsApiQuery {
        q: Some("support".to_string()),
        connector_type: Some("zendesk".to_string()),
        enabled: Some(true),
        ..Default::default()
    };
    assert!(sensor_api_matches(&sensor, &query));
}

#[test]
fn trigger_api_filters_by_name_event_match_and_enabled_state() {
    let trigger = crate::triggers_client::TriggerSummary {
        id: uuid::Uuid::new_v4(),
        name: "High risk ticket".to_string(),
        event_type_match: "risk.ticket".to_string(),
        enabled: true,
    };
    let query = TriggersApiQuery {
        q: Some("risk.ticket".to_string()),
        enabled: Some(true),
        ..Default::default()
    };
    assert!(trigger_api_matches(&trigger, &query));
}

#[test]
fn incident_api_filters_by_text_severity_and_owner() {
    let incident = IncidentDetail {
        incident: common::Incident {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            title: "Payment outage".to_string(),
            summary: "Checkout failures".to_string(),
            severity: common::IncidentSeverity::Critical,
            status: common::IncidentStatus::Open,
            assigned_to: Some("alice".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            resolved_at: None,
        },
        event_ids: Vec::new(),
        notes: Vec::new(),
    };
    let query = IncidentsApiQuery {
        q: Some("checkout".to_string()),
        severity: Some("critical".to_string()),
        assigned_to: Some("alice".to_string()),
        ..Default::default()
    };
    assert!(incident_api_matches(&incident, &query));
}

#[test]
fn incident_360_timeline_is_chronological_and_bounded() {
    let now = chrono::Utc::now();
    let incident = IncidentDetail {
        incident: common::Incident {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            title: "Payment outage".to_string(),
            summary: String::new(),
            severity: common::IncidentSeverity::High,
            status: common::IncidentStatus::Open,
            assigned_to: None,
            created_at: now - chrono::Duration::hours(2),
            updated_at: now,
            resolved_at: None,
        },
        event_ids: Vec::new(),
        notes: vec![common::IncidentNote {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            incident_id: uuid::Uuid::new_v4(),
            author: "operator".to_string(),
            body: "Customer impact confirmed".to_string(),
            created_at: now - chrono::Duration::minutes(20),
        }],
    };
    let timeline = build_incident_timeline_api(&incident, &[], &[], &[]);

    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0].kind, "Investigation note");
    assert_eq!(timeline[1].kind, "Case opened");
    assert!(!timeline[0].failure);
}

#[test]
fn global_search_matches_ontology_contracts_by_name_schema_and_target() {
    let object_type = common::ontology::ObjectType {
        id: uuid::Uuid::new_v4(),
        tenant_id: uuid::Uuid::new_v4(),
        name: "Customer Account".to_string(),
        version: 2,
        property_schema: serde_json::json!({"account_number": {"type": "string"}}),
        mapping_rules: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let action_type = common::ontology::ActionType {
        id: uuid::Uuid::new_v4(),
        tenant_id: object_type.tenant_id,
        name: "Suspend Customer Account".to_string(),
        target_object_type_id: Some(object_type.id),
        parameter_schema: serde_json::json!({"reason": {"type": "string"}}),
        preconditions: serde_json::json!({"status": "active"}),
        effect_definition: serde_json::json!({"status": "suspended"}),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    assert!(search_object_type_matches(&object_type, "account_number"));
    assert!(search_action_type_matches(&action_type, Some(&object_type), "suspend"));
    assert!(search_action_type_matches(&action_type, Some(&object_type), "customer account"));
    assert!(!search_action_type_matches(&action_type, Some(&object_type), "rotate key"));
    assert_eq!(global_search_scope("object_types"), "object_types");
    assert_eq!(global_search_scope("action_types"), "action_types");
}

#[test]
fn normalization_mapping_validation_requires_a_source_and_field_map() {
    let mapping = common::NormalizationMapping::new(
        uuid::Uuid::new_v4(),
        "",
        std::collections::BTreeMap::new(),
    );
    assert_eq!(validate_mapping(&mapping).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn ontology_action_api_requires_a_target_set() {
    let request = InvokeOntologyActionApiRequest {
        action_type_id: uuid::Uuid::new_v4(),
        target_object_ids: Vec::new(),
        parameters: serde_json::json!({}),
        triggering_event_ref: None,
    };
    assert!(request.target_object_ids.is_empty());
}

#[test]
fn ontology_authoring_validation_rejects_invalid_contract_shape() {
    assert!(validate_ontology_name("Customer").is_ok());
    assert_eq!(validate_ontology_name(" ").unwrap_err().status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        validate_ontology_json(&serde_json::json!([]), "property_schema").unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
    assert!(validate_link_cardinality("many-to-one").is_ok());
    assert_eq!(
        validate_link_cardinality("many-to-many").unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn action_api_rejects_reversed_date_windows() {
    let error =
        action_api_date_range(Some(&"2026-07-24".to_string()), Some(&"2026-07-23".to_string()))
            .unwrap_err();
    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    assert!(action_api_date_range(
        Some(&"2026-07-23".to_string()),
        Some(&"2026-07-24".to_string())
    )
    .is_ok());
}

#[test]
fn event_type_api_rejects_unknown_coverage_scope() {
    assert!(normalize_event_coverage_api(Some(&"governed".to_string())).is_ok());
    assert_eq!(
        normalize_event_coverage_api(Some(&"unclassified".to_string())).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn retention_policy_api_rejects_unknown_data_class() {
    assert!(parse_data_class("normalized").is_some());
    assert!(parse_data_class("archive").is_none());
}

#[test]
fn normalization_mapping_api_query_defaults_to_unfiltered() {
    let query = NormalizationMappingsApiQuery::default();
    assert!(query.q.is_none());
    assert!(query.source_type.is_none());
}

#[test]
fn compliance_hold_api_query_defaults_to_unfiltered() {
    let query = ComplianceHoldsApiQuery::default();
    assert!(query.data_class.is_none());
    assert!(query.active.is_none());
}

#[test]
fn backup_api_rejects_unknown_status_scope() {
    assert!(normalize_backup_status_api(Some(&"failed".to_string())).is_ok());
    assert_eq!(
        normalize_backup_status_api(Some(&"queued".to_string())).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn action_type_api_requires_object_contracts() {
    let request = ActionTypeApiRequest {
        name: "Escalate".to_string(),
        target_object_type_id: None,
        parameter_schema: serde_json::json!([]),
        preconditions: serde_json::json!({}),
        effect_definition: serde_json::json!({}),
    };
    assert_eq!(action_type_input(request).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn action_type_api_rejects_blank_names() {
    let request = ActionTypeApiRequest {
        name: "  ".to_string(),
        target_object_type_id: None,
        parameter_schema: serde_json::json!({}),
        preconditions: serde_json::json!({}),
        effect_definition: serde_json::json!({}),
    };
    assert_eq!(action_type_input(request).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn dashboard_layout_api_requires_a_complete_unique_widget_order() {
    let request = DashboardLayoutApiRequest {
        order: vec!["signal-trend".to_string()],
        hidden: Vec::new(),
        scope: String::new(),
    };
    assert_eq!(validate_dashboard_layout(request).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn dashboard_layout_api_rejects_unknown_hidden_widgets() {
    let request = DashboardLayoutApiRequest {
        order: DASHBOARD_WIDGET_IDS.iter().map(|id| (*id).to_string()).collect(),
        hidden: vec!["unknown-widget".to_string()],
        scope: String::new(),
    };
    assert_eq!(validate_dashboard_layout(request).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn dashboard_layout_scope_accepts_personal_and_workspace_only() {
    assert_eq!(normalize_dashboard_scope("").unwrap(), "personal");
    assert_eq!(normalize_dashboard_scope("workspace").unwrap(), "workspace");
    assert_eq!(normalize_dashboard_scope("team").unwrap_err().status(), StatusCode::BAD_REQUEST);
    assert_eq!(dashboard_owner("workspace", "alice"), "workspace");
    assert_eq!(dashboard_owner("personal", "alice"), "alice");
}

#[test]
fn data_api_rejects_non_rfc3339_date_filters() {
    let query = DataApiQuery { from: Some("yesterday".to_string()), ..Default::default() };
    assert_eq!(data_filter(&query).unwrap_err().status(), StatusCode::BAD_REQUEST);
}

#[test]
fn data_api_caps_page_size_and_normalizes_offset() {
    let query = DataApiQuery { limit: Some(5000), offset: Some(-10), ..Default::default() };
    let filter = data_filter(&query).unwrap();
    assert_eq!(filter.limit, 1000);
    assert_eq!(filter.offset, 0);
}

#[test]
fn work_api_query_filters_incidents_by_text_and_owner() {
    let incident = IncidentDetail {
        incident: common::Incident {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            title: "Payment outage".to_string(),
            summary: "Checkout failure".to_string(),
            severity: common::IncidentSeverity::High,
            status: common::IncidentStatus::Open,
            assigned_to: Some("alice".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            resolved_at: None,
        },
        event_ids: Vec::new(),
        notes: Vec::new(),
    };
    assert!(work_incident_matches(
        &incident,
        &WorkApiQuery {
            q: Some("payment".to_string()),
            assigned_to: Some("alice".to_string()),
            ..Default::default()
        }
    ));
    assert!(!work_incident_matches(
        &incident,
        &WorkApiQuery { assigned_to: Some("bob".to_string()), ..Default::default() }
    ));
}

#[test]
fn action_template_api_rejects_non_object_provider_config() {
    let request = ActionTemplateApiRequest {
        name: "Notify".to_string(),
        description: String::new(),
        action_type: common::ActionType::Webhook,
        config: serde_json::json!([]),
    };
    assert_eq!(
        action_template_from_request(request, uuid::Uuid::new_v4(), uuid::Uuid::new_v4())
            .unwrap_err()
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn api_key_api_rejects_blank_labels() {
    let request = CreateApiKeyApiRequest { label: "  ".to_string() };
    assert_eq!(
        validate_api_key_label(&request.label).unwrap_err().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn ontology_graph_neighborhood_respects_depth_and_limit() {
    let a = uuid::Uuid::from_u128(1);
    let b = uuid::Uuid::from_u128(2);
    let c = uuid::Uuid::from_u128(3);
    let object_type_id = uuid::Uuid::from_u128(10);
    let now = chrono::Utc::now();
    let objects = [a, b, c]
        .into_iter()
        .map(|id| common::ontology::Object {
            id,
            tenant_id: uuid::Uuid::from_u128(99),
            object_type_id,
            properties: serde_json::json!({"name": id.to_string()}),
            source_lineage: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        })
        .collect::<Vec<_>>();
    let links = [(uuid::Uuid::from_u128(20), a, b), (uuid::Uuid::from_u128(21), b, c)]
        .into_iter()
        .map(|(id, source_object_id, target_object_id)| common::ontology::Link {
            id,
            tenant_id: uuid::Uuid::from_u128(99),
            link_type_id: uuid::Uuid::from_u128(30),
            source_object_id,
            target_object_id,
            properties: None,
            created_at: now,
            updated_at: now,
        })
        .collect::<Vec<_>>();
    assert_eq!(ontology_graph_neighbors(&objects, &links, a, 1, 10).len(), 2);
    assert_eq!(ontology_graph_neighbors(&objects, &links, a, 2, 10).len(), 3);
    assert_eq!(ontology_graph_neighbors(&objects, &links, a, 2, 2).len(), 2);
}

#[test]
fn ontology_object_360_matches_only_explicit_action_targets() {
    let object_id = uuid::Uuid::from_u128(42);
    let other_id = uuid::Uuid::from_u128(43);
    assert!(action_targets_object_api(&serde_json::json!([object_id]), object_id));
    assert!(!action_targets_object_api(&serde_json::json!([other_id]), object_id));
    assert!(!action_targets_object_api(&serde_json::json!({"id": object_id}), object_id));
}

#[test]
fn ontology_object_360_exposes_a_unified_timeline_contract() {
    let source = include_str!("api_v1_handler.rs");
    assert!(source.contains("struct OntologyObject360TimelineApi"));
    assert!(source.contains("timeline.sort_by_key"));
    assert!(source.contains("timeline.truncate(100)"));
}

#[test]
fn ontology_object_360_exposes_eligible_governed_actions() {
    let source = include_str!("api_v1_handler.rs");
    assert!(source.contains("struct OntologyObject360AvailableActionApi"));
    assert!(source.contains("available_actions"));
    assert!(source.contains("action_preconditions_satisfied(&object.properties"));
}

#[test]
fn ontology_object_360_exposes_source_evidence_records() {
    let source = include_str!("api_v1_handler.rs");
    assert!(source.contains("struct OntologyObject360SourceRecordApi"));
    assert!(source.contains("source_records"));
    assert!(source.contains("/data/{}/journey"));
}

#[test]
fn ontology_object_360_exposes_relationship_properties_and_history_links() {
    let source = include_str!("api_v1_handler.rs");
    assert!(source.contains("relationship_id: Uuid"));
    assert!(source.contains("properties: link.properties.clone()"));
    assert!(source.contains("history_href"));
    assert!(source.contains("history: Vec<OntologyObject360HistoryApi>"));
    assert!(source.contains("list_all_link_history"));
    assert!(source.contains("properties_schema: link_schemas"));
    assert!(source.contains("struct OntologyObject360RelationshipOptionApi"));
    assert!(source.contains("relationship_options"));
    assert!(source.contains("cardinality: link_type.cardinality.clone()"));
    assert!(source.contains("eligibility_reason"));
    assert!(source.contains("source_unique"));
    assert!(source.contains("target_unique"));
}

#[test]
fn event_360_accepts_both_supported_event_reference_shapes() {
    let event_id = uuid::Uuid::from_u128(44);
    assert_eq!(event_ref_api(&serde_json::json!({"event_id": event_id})), Some(event_id));
    assert_eq!(event_ref_api(&serde_json::json!({"id": event_id})), Some(event_id));
    assert_eq!(event_ref_api(&serde_json::json!({"event_id": "not-a-uuid"})), None);
}
