use super::*;

fn connector() -> FabricConnector {
    FabricConnector::new(
        "fabric",
        "localhost",
        14330,
        "master",
        "http://token-url",
        "client-id",
        "client-secret",
        "SELECT 1",
        true,
    )
}

#[test]
fn reports_its_own_connector_id() {
    assert_eq!(connector().connector_id(), "fabric");
}

#[test]
fn reports_fabric_record_as_its_source_type() {
    assert_eq!(connector().source_type(), SourceType::FabricRecord);
}
