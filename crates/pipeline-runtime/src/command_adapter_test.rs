use super::*;

#[test]
fn command_dispatch_requires_idempotency_and_concurrency_values() {
    let dispatch = CommandDispatch {
        endpoint: "https://erp.example.test/orders/42".to_string(),
        idempotency_key: "order-42:approve".to_string(),
        expected_version: "W/\"41\"".to_string(),
        payload: serde_json::json!({"approved": true}),
    };
    assert!(dispatch.is_valid());
}
