use super::*;
use crate::analysis_client::analysis_client_test::{FailingAnalysisClient, InMemoryAnalysisClient};
use crate::event_publisher::event_publisher_test::{FailingEventPublisher, InMemoryEventPublisher};
use common::SourceType;
use serde_json::json;

fn record_for(tenant_id: Uuid) -> RawRecord {
    RawRecord::new("zendesk", SourceType::Ticket, tenant_id, json!({"description": "hi"}))
}

#[test]
fn group_by_tenant_splits_mixed_batch_into_per_tenant_groups() {
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let records = vec![record_for(tenant_a), record_for(tenant_b), record_for(tenant_a)];

    let groups = group_by_tenant(records);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[&tenant_a].len(), 2);
    assert_eq!(groups[&tenant_b].len(), 1);
}

#[test]
fn group_by_tenant_on_empty_input_yields_no_groups() {
    assert!(group_by_tenant(vec![]).is_empty());
}

#[tokio::test]
async fn process_batch_calls_analysis_once_and_publishes_one_message_per_record() {
    let analysis_client = Arc::new(InMemoryAnalysisClient::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let deps =
        AnalysisDeps { analysis_client: analysis_client.clone(), publisher: publisher.clone() };
    let tenant_id = Uuid::new_v4();
    let records = vec![record_for(tenant_id), record_for(tenant_id)];

    let published = process_batch(&deps, tenant_id, records).await.unwrap();

    assert_eq!(published, 2);
    assert_eq!(analysis_client.calls.lock().unwrap().len(), 1, "must be a single batch call");
    assert_eq!(analysis_client.calls.lock().unwrap()[0], (tenant_id, 2));
    assert_eq!(publisher.published.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn process_batch_on_empty_records_is_a_no_op() {
    let analysis_client = Arc::new(InMemoryAnalysisClient::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let deps = AnalysisDeps { analysis_client: analysis_client.clone(), publisher };

    let published = process_batch(&deps, Uuid::new_v4(), vec![]).await.unwrap();

    assert_eq!(published, 0);
    assert!(analysis_client.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn process_batch_propagates_analysis_failure() {
    let deps = AnalysisDeps {
        analysis_client: Arc::new(FailingAnalysisClient),
        publisher: Arc::new(InMemoryEventPublisher::default()),
    };
    let tenant_id = Uuid::new_v4();

    let err = process_batch(&deps, tenant_id, vec![record_for(tenant_id)]).await.unwrap_err();
    assert!(matches!(err, BatchError::Analysis(_)));
}

#[tokio::test]
async fn process_batch_continues_past_individual_publish_failures() {
    let analysis_client = Arc::new(InMemoryAnalysisClient::default());
    let deps = AnalysisDeps { analysis_client, publisher: Arc::new(FailingEventPublisher) };
    let tenant_id = Uuid::new_v4();

    let published =
        process_batch(&deps, tenant_id, vec![record_for(tenant_id), record_for(tenant_id)])
            .await
            .unwrap();

    assert_eq!(
        published, 0,
        "publish failures are logged, not fatal, but nothing was actually published"
    );
}
