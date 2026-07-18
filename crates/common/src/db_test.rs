use super::*;

#[test]
fn valid_schema_names_are_accepted() {
    assert!(is_valid_schema_name("ingestion_service"));
    assert!(is_valid_schema_name("a"));
    assert!(is_valid_schema_name("_private"));
    assert!(is_valid_schema_name("service_2"));
}

#[test]
fn invalid_schema_names_are_rejected() {
    assert!(!is_valid_schema_name(""), "empty name");
    assert!(!is_valid_schema_name("2service"), "leading digit");
    assert!(!is_valid_schema_name("Service"), "uppercase");
    assert!(!is_valid_schema_name("service-name"), "hyphen");
    assert!(!is_valid_schema_name("service; DROP TABLE users;--"), "injection attempt");
    assert!(!is_valid_schema_name("service name"), "space");
    assert!(!is_valid_schema_name(&"a".repeat(64)), "over postgres's 63-byte identifier limit");
}

#[tokio::test]
async fn rejects_before_attempting_to_connect_when_schema_name_is_invalid() {
    let result =
        connect_with_schema("postgres://ignored:ignored@127.0.0.1:1/ignored", "bad-name").await;
    assert!(matches!(result, Err(ConnectError::InvalidSchemaName(_))));
}
