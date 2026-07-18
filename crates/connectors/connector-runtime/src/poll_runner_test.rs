use super::*;
use crate::ingestion_client::ingestion_client_test::{
    FailingIngestionClient, InMemoryIngestionClient,
};
use async_trait::async_trait;
use common::connector::ConnectorError;
use common::{RawRecord, SourceType};

struct StubConnector {
    records: Vec<RawRecord>,
}

#[async_trait]
impl Connector for StubConnector {
    fn connector_id(&self) -> &str {
        "stub"
    }
    fn source_type(&self) -> SourceType {
        SourceType::Generic
    }
    async fn poll(&self, _tenant_id: Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        Ok(self.records.clone())
    }
}

struct FailingConnector;

#[async_trait]
impl Connector for FailingConnector {
    fn connector_id(&self) -> &str {
        "failing"
    }
    fn source_type(&self) -> SourceType {
        SourceType::Generic
    }
    async fn poll(&self, _tenant_id: Uuid) -> Result<Vec<RawRecord>, ConnectorError> {
        Err(ConnectorError::SourceUnavailable("simulated".to_string()))
    }
}

fn sample_record(tenant_id: Uuid) -> RawRecord {
    RawRecord::new("stub", SourceType::Generic, tenant_id, serde_json::json!({}))
}

#[tokio::test]
async fn ingests_every_polled_record() {
    let tenant_id = Uuid::new_v4();
    let connector =
        StubConnector { records: vec![sample_record(tenant_id), sample_record(tenant_id)] };
    let ingestion_client = InMemoryIngestionClient::default();

    let summary = run_poll_cycle(&connector, tenant_id, &ingestion_client).await.unwrap();

    assert_eq!(summary, PollSummary { polled: 2, ingested: 2, failed: 0 });
    assert_eq!(ingestion_client.ingested.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_failed_ingest_is_counted_but_does_not_abort_the_cycle() {
    let tenant_id = Uuid::new_v4();
    let connector = StubConnector { records: vec![sample_record(tenant_id)] };
    let ingestion_client = FailingIngestionClient;

    let summary = run_poll_cycle(&connector, tenant_id, &ingestion_client).await.unwrap();

    assert_eq!(summary, PollSummary { polled: 1, ingested: 0, failed: 1 });
}

#[tokio::test]
async fn a_poll_failure_returns_an_error() {
    let tenant_id = Uuid::new_v4();
    let connector = FailingConnector;
    let ingestion_client = InMemoryIngestionClient::default();

    let err = run_poll_cycle(&connector, tenant_id, &ingestion_client).await.unwrap_err();
    assert!(matches!(err, PollRunError::Poll(_)));
}

#[tokio::test]
async fn an_empty_poll_result_is_a_no_op_success() {
    let tenant_id = Uuid::new_v4();
    let connector = StubConnector { records: vec![] };
    let ingestion_client = InMemoryIngestionClient::default();

    let summary = run_poll_cycle(&connector, tenant_id, &ingestion_client).await.unwrap();

    assert_eq!(summary, PollSummary::default());
}
