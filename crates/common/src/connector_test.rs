use super::*;
use serde_json::json;

struct StubConnector {
    id: String,
    records: Vec<RawRecord>,
}

#[async_trait]
impl Connector for StubConnector {
    fn connector_id(&self) -> &str {
        &self.id
    }

    fn source_type(&self) -> SourceType {
        SourceType::Generic
    }

    async fn poll(&self, _tenant_id: Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        Ok(self.records.clone())
    }
}

#[tokio::test]
async fn poll_returns_configured_records() {
    let tenant_id = Uuid::new_v4();
    let record = RawRecord::new("stub", SourceType::Generic, tenant_id, json!({}));
    let connector = StubConnector { id: "stub".to_string(), records: vec![record.clone()] };

    let result = connector.poll(tenant_id).await.unwrap();
    assert_eq!(result, vec![record]);
    assert_eq!(connector.connector_id(), "stub");
    assert_eq!(connector.source_type(), SourceType::Generic);
}

#[test]
fn connector_error_messages_are_descriptive() {
    let err = ConnectorError::RateLimited { retry_after_secs: 30 };
    assert!(err.to_string().contains("30"));

    let err = ConnectorError::AuthFailed("bad token".to_string());
    assert!(err.to_string().contains("bad token"));
}
