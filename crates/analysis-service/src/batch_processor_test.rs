use super::*;
use crate::analysis_client::analysis_client_test::{
    spawn_stub_chat_completions, FailingAnalysisClient, InMemoryAnalysisClient,
};
use crate::analysis_config_repository::analysis_config_repository_test::InMemoryAnalysisConfigRepository;
use crate::event_publisher::event_publisher_test::{FailingEventPublisher, InMemoryEventPublisher};
use common::{AnalysisConfig, AnalysisProvider, SourceType};
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
    let deps = AnalysisDeps {
        analysis_client: analysis_client.clone(),
        publisher: publisher.clone(),
        analysis_config_repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        http_client: reqwest::Client::new(),
    };
    let tenant_id = Uuid::new_v4();
    let records = vec![record_for(tenant_id), record_for(tenant_id)];

    let published = process_batch(&deps, tenant_id, records).await.unwrap();

    assert_eq!(published, 2);
    assert_eq!(analysis_client.calls.lock().unwrap().len(), 1, "must be a single batch call");
    assert_eq!(analysis_client.calls.lock().unwrap()[0], (tenant_id, 2, None));
    assert_eq!(publisher.published.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn process_batch_passes_the_tenants_configured_prompt_to_the_analysis_client() {
    let analysis_client = Arc::new(InMemoryAnalysisClient::default());
    let config_repository = Arc::new(InMemoryAnalysisConfigRepository::default());
    let tenant_id = Uuid::new_v4();
    config_repository
        .upsert(AnalysisConfig::new(tenant_id, "look for urgent tickets"))
        .await
        .unwrap();
    let deps = AnalysisDeps {
        analysis_client: analysis_client.clone(),
        publisher: Arc::new(InMemoryEventPublisher::default()),
        analysis_config_repository: config_repository,
        http_client: reqwest::Client::new(),
    };

    process_batch(&deps, tenant_id, vec![record_for(tenant_id)]).await.unwrap();

    assert_eq!(
        analysis_client.calls.lock().unwrap()[0],
        (tenant_id, 1, Some("look for urgent tickets".to_string()))
    );
}

#[tokio::test]
async fn process_batch_on_empty_records_is_a_no_op() {
    let analysis_client = Arc::new(InMemoryAnalysisClient::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let deps = AnalysisDeps {
        analysis_client: analysis_client.clone(),
        publisher,
        analysis_config_repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        http_client: reqwest::Client::new(),
    };

    let published = process_batch(&deps, Uuid::new_v4(), vec![]).await.unwrap();

    assert_eq!(published, 0);
    assert!(analysis_client.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn process_batch_propagates_analysis_failure() {
    let deps = AnalysisDeps {
        analysis_client: Arc::new(FailingAnalysisClient),
        publisher: Arc::new(InMemoryEventPublisher::default()),
        analysis_config_repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        http_client: reqwest::Client::new(),
    };
    let tenant_id = Uuid::new_v4();

    let err = process_batch(&deps, tenant_id, vec![record_for(tenant_id)]).await.unwrap_err();
    assert!(matches!(err, BatchError::Analysis(_)));
}

#[tokio::test]
async fn process_batch_continues_past_individual_publish_failures() {
    let analysis_client = Arc::new(InMemoryAnalysisClient::default());
    let deps = AnalysisDeps {
        analysis_client,
        publisher: Arc::new(FailingEventPublisher),
        analysis_config_repository: Arc::new(InMemoryAnalysisConfigRepository::default()),
        http_client: reqwest::Client::new(),
    };
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

#[tokio::test]
async fn process_batch_routes_to_the_openai_compatible_client_when_a_tenant_is_configured_for_it() {
    let (endpoint, _captured) =
        spawn_stub_chat_completions(r#"{"sentiment": -0.5}"#.to_string()).await;
    let default_client = Arc::new(InMemoryAnalysisClient::default());
    let publisher = Arc::new(InMemoryEventPublisher::default());
    let config_repository = Arc::new(InMemoryAnalysisConfigRepository::default());
    let tenant_id = Uuid::new_v4();
    let mut config = AnalysisConfig::new(tenant_id, "flag urgent issues");
    config.provider = AnalysisProvider::OpenAiCompatible;
    config.endpoint = Some(endpoint);
    config.model = Some("qwen3:8b".to_string());
    config_repository.upsert(config).await.unwrap();
    let deps = AnalysisDeps {
        analysis_client: default_client.clone(),
        publisher: publisher.clone(),
        analysis_config_repository: config_repository,
        http_client: reqwest::Client::new(),
    };

    let published = process_batch(&deps, tenant_id, vec![record_for(tenant_id)]).await.unwrap();

    assert_eq!(published, 1);
    assert!(
        default_client.calls.lock().unwrap().is_empty(),
        "the platform-default Foundry client must not have been called for this tenant"
    );
    assert_eq!(publisher.published.lock().unwrap()[0].analysis, json!({"sentiment": -0.5}));
}
