//! Integration test against a real RabbitMQ management API (CLAUDE.md §2). Requires
//! RABBITMQ_MANAGEMENT_URL (e.g. http://kizashi:kizashi@localhost:15672).

use observability::{BacklogReader, RabbitMqManagementBacklogReader, PIPELINE_QUEUES};

fn test_reader() -> RabbitMqManagementBacklogReader {
    let management_url = std::env::var("RABBITMQ_MANAGEMENT_URL")
        .expect("RABBITMQ_MANAGEMENT_URL must be set to run this test");
    RabbitMqManagementBacklogReader::new(reqwest::Client::new(), management_url, "/".to_string())
}

#[tokio::test]
async fn queue_depths_returns_one_entry_per_pipeline_stage_against_real_rabbitmq() {
    let reader = test_reader();

    let depths = reader.queue_depths().await.expect("queue_depths should succeed");

    assert_eq!(depths.len(), PIPELINE_QUEUES.len());
    for (stage, queue_name) in PIPELINE_QUEUES {
        let found = depths.iter().find(|d| d.stage == *stage).unwrap();
        assert_eq!(found.queue_name, *queue_name);
    }
}
