use super::validate_properties;
use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

#[test]
fn rich_property_contract_accepts_governed_operational_values() {
    let account_id = Uuid::new_v4();
    let schema = json!({
        "account_id": {"type": "uuid", "required": true, "source_ownership": "external", "write_policy": "read_only"},
        "balance": {"type": "money", "required": true, "sensitivity": "financial"},
        "opened_on": {"type": "date"},
        "synced_at": {"type": "timestamp"},
        "logo": {"type": "image"},
        "embedding": {"type": "vector", "dimensions": 3},
        "location": {"type": "coordinate"},
        "state": {"type": "enum", "values": ["open", "closed"]},
        "owner": {"type": "reference", "target_type": "Contact"},
        "tags": {"type": "array", "items": {"type": "string"}},
        "address": {"type": "object", "properties": {"city": {"type": "string", "required": true}}}
    });
    let properties = json!({
        "account_id": account_id.to_string(),
        "balance": {"amount": "1234.50", "currency": "USD"},
        "opened_on": "2026-07-25",
        "synced_at": "2026-07-25T12:30:00Z",
        "logo": {"uri": "s3://tenant/logo.png", "sha256": "abc", "content_type": "image/png"},
        "embedding": [0.1, 0.2, 0.3],
        "location": {"latitude": 40.7128, "longitude": -74.0060},
        "state": "open",
        "owner": account_id.to_string(),
        "tags": ["priority", "customer"],
        "address": {"city": "New York"}
    });

    assert!(validate_properties(&schema, &properties).is_ok());
}

#[test]
fn rich_property_contract_rejects_invalid_semantic_values_and_constraints() {
    let schema = json!({
        "state": {"type": "enum", "values": ["open", "closed"], "required": true},
        "embedding": {"type": "vector", "dimensions": 2},
        "document": {"type": "file"},
        "location": {"type": "coordinate"}
    });

    for properties in [
        json!({"state": "pending", "embedding": [0.1, 0.2], "document": {"uri":"s3://x", "sha256":"a", "content_type":"application/pdf"}, "location":{"latitude":0,"longitude":0}}),
        json!({"state": "open", "embedding": [0.1], "document": {"uri":"s3://x", "sha256":"a", "content_type":"application/pdf"}, "location":{"latitude":0,"longitude":0}}),
        json!({"state": "open", "embedding": [0.1, 0.2], "document": {"uri":"s3://x", "content_type":"application/pdf"}, "location":{"latitude":0,"longitude":0}}),
        json!({"state": "open", "embedding": [0.1, 0.2], "document": {"uri":"s3://x", "sha256":"a", "content_type":"application/pdf"}, "location":{"latitude":91,"longitude":0}}),
    ] {
        assert_eq!(validate_properties(&schema, &properties), Err(StatusCode::BAD_REQUEST));
    }
}

#[test]
fn legacy_primitive_property_contracts_remain_valid() {
    let schema =
        json!({"name": {"type": "string", "required": true}, "count": {"type": "integer"}});
    assert!(validate_properties(&schema, &json!({"name": "Northwind", "count": 2})).is_ok());
}

#[test]
fn property_schema_rejects_malformed_rich_definitions_before_persistence() {
    assert!(super::validate_schema(&json!({
        "total": {"type": "money", "display": {"label": "Total"}, "source_ownership": "external"},
        "embedding": {"type": "vector", "dimensions": 384},
        "labels": {"type": "array", "items": {"type": "enum", "values": ["vip", "watch"]}}
    }))
    .is_ok());

    for schema in [
        json!({"state": {"type": "enum"}}),
        json!({"embedding": {"type": "vector", "dimensions": 0}}),
        json!({"labels": {"type": "array", "items": {"type": "not-a-type"}}}),
        json!({"owner": {"type": "reference", "source_ownership": 12}}),
    ] {
        assert_eq!(super::validate_schema(&schema), Err(StatusCode::BAD_REQUEST));
    }
}
