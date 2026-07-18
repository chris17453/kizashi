use super::*;
use crate::archive_store::archive_store_test::InMemoryArchiveStore;
use crate::raw_record_client::raw_record_client_test::InMemoryRawRecordClient;
use common::{RawRecord, SourceType};
use uuid::Uuid;

fn sample_record(tenant_id: Uuid) -> RawRecord {
    RawRecord::new("zendesk", SourceType::Ticket, tenant_id, serde_json::json!({}))
}

#[tokio::test]
async fn reimports_every_record_in_the_batch() {
    let tenant_id = Uuid::new_v4();
    let records = vec![sample_record(tenant_id), sample_record(tenant_id)];
    let archive_store = Arc::new(InMemoryArchiveStore::default());
    let key = archive_store
        .write_batch(tenant_id, "raw", &records, chrono::Utc::now(), chrono::Utc::now())
        .await
        .unwrap();
    let record_client = Arc::new(InMemoryRawRecordClient::default());

    let state = ReimportState { archive_store, record_client: record_client.clone() };
    let summary = reimport(&state, &key).await.unwrap();

    assert_eq!(summary.records_reimported, 2);
    assert_eq!(summary.records_failed, 0);
    assert_eq!(record_client.reimported.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn unknown_archive_key_returns_an_error() {
    let archive_store = Arc::new(InMemoryArchiveStore::default());
    let record_client = Arc::new(InMemoryRawRecordClient::default());
    let state = ReimportState { archive_store, record_client };

    let err = reimport(&state, "missing-key").await.unwrap_err();
    assert!(matches!(err, ReimportError::Archive(_)));
}

#[tokio::test]
async fn a_failed_record_reimport_is_counted_but_does_not_abort_the_batch() {
    use crate::archive_store::ArchiveStore;
    use crate::raw_record_client::{RawRecordClient, RawRecordClientError};
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};

    struct FailFirstThenSucceed {
        calls: std::sync::Mutex<u32>,
    }
    #[async_trait]
    impl RawRecordClient for FailFirstThenSucceed {
        async fn list_older_than(
            &self,
            _tenant_id: Uuid,
            _cutoff: DateTime<Utc>,
            _limit: i64,
        ) -> Result<Vec<RawRecord>, RawRecordClientError> {
            Ok(vec![])
        }
        async fn delete(
            &self,
            _tenant_id: Uuid,
            _record_id: Uuid,
        ) -> Result<(), RawRecordClientError> {
            Ok(())
        }
        async fn reimport(&self, _record: &RawRecord) -> Result<(), RawRecordClientError> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == 1 {
                Err(RawRecordClientError::Rejected(500))
            } else {
                Ok(())
            }
        }
    }

    let tenant_id = Uuid::new_v4();
    let records = vec![sample_record(tenant_id), sample_record(tenant_id)];
    let archive_store = Arc::new(InMemoryArchiveStore::default());
    let key = archive_store
        .write_batch(tenant_id, "raw", &records, chrono::Utc::now(), chrono::Utc::now())
        .await
        .unwrap();
    let record_client = Arc::new(FailFirstThenSucceed { calls: std::sync::Mutex::new(0) });

    let state = ReimportState { archive_store, record_client };
    let summary = reimport(&state, &key).await.unwrap();

    assert_eq!(summary.records_reimported, 1);
    assert_eq!(summary.records_failed, 1);
}
