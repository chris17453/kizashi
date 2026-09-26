use super::*;
use chrono::Utc;
use uuid::Uuid;

fn event() -> Event {
    Event {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        event_type: "risk.detected".to_string(),
        source_connector_ids: vec![],
        record_ids: vec![],
        entity_ref: "customer-42".to_string(),
        group_key: "customer-42".to_string(),
        payload: serde_json::json!({"score": 0.97}),
        occurred_at: Utc::now(),
        created_at: Utc::now(),
        status: common::EventStatus::New,
    }
}

#[test]
fn pdf_has_a_real_header_and_escaped_text() {
    let bytes = build_pdf("hello (world)");
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert!(String::from_utf8_lossy(&bytes).contains("hello \\(world\\)"));
}

#[test]
fn xlsx_is_excel_xml_and_escapes_cells() {
    let bytes = build_xlsx("a < b");
    let value = String::from_utf8(bytes).unwrap();
    assert!(value.starts_with("<?xml"));
    assert!(value.contains("a &lt; b"));
}

#[tokio::test]
async fn artifact_dispatch_rejects_missing_delivery_url() {
    let action = ActionRef { action_type: ActionType::GeneratePdf, config: serde_json::json!({}) };
    let error = ArtifactActionDispatcher::new(None).dispatch(&action, &event()).await.unwrap_err();
    assert!(matches!(error, DispatchError::MissingUrl));
}
