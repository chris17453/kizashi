use super::SessionContext;
use common::Role;
use uuid::Uuid;

#[test]
fn session_context_serializes_only_the_shell_identity_claims() {
    let context = SessionContext {
        username: "operator".to_string(),
        role: Role::Operator,
        tenant_id: Uuid::nil(),
        workspace: "acme".to_string(),
    };
    let value = serde_json::to_value(context).unwrap();
    assert_eq!(value["username"], "operator");
    assert_eq!(value["role"], "operator");
    assert_eq!(value["tenant_id"], Uuid::nil().to_string());
    assert!(value.get("bearer_token").is_none());
}
