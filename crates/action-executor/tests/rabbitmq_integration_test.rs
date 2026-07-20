//! Integration test against real RabbitMQ and real Postgres (CLAUDE.md §2), plus real
//! in-process HTTP servers standing in for Trigger Engine and the webhook target an action
//! dispatches to. Mirrors the pattern already proven in
//! `normalization-service/tests/normalization_integration_test.rs`: publish a message to the
//! real exchange `main.rs` consumes from, consume it with a test consumer, then exercise the
//! crate's own processing function (`process_event`) directly against real infra and observe
//! what it wrote. Requires RABBITMQ_URL and DATABASE_URL.

use action_executor::{
    process_event, ActionDeps, ExecutionRepository, HttpTriggerClient, PostgresExecutionRepository,
    RoutingActionDispatcher, EVENT_CREATED_EXCHANGE,
};
use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use common::{
    ActionExecutionStatus, ActionRef, ActionType, Event, EventStatus, TriggerCondition,
    TriggerDefinition,
};
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, ExchangeDeclareOptions,
    QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, ExchangeKind};
use std::sync::Arc;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "action_executor")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");
    pool
}

async fn test_channel() -> lapin::Channel {
    let rabbitmq_url =
        std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set to run this test");
    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    connection.create_channel().await.expect("failed to open channel")
}

/// Stands in for Trigger Engine's `GET /v1/triggers/:id` — returns a fixed webhook-action
/// trigger for any id, since this test only cares that Action Executor calls it and then
/// dispatches, not Trigger Engine's own persistence behavior.
async fn spawn_stub_trigger_engine(
    trigger_id: Uuid,
    tenant_id: Uuid,
    webhook_url: String,
) -> String {
    async fn handler(
        Path(_id): Path<Uuid>,
        axum::extract::State(trigger): axum::extract::State<TriggerDefinition>,
    ) -> impl IntoResponse {
        Json(trigger)
    }
    let trigger = TriggerDefinition {
        id: trigger_id,
        tenant_id,
        name: "integration-test-trigger".to_string(),
        event_type_match: "sentiment_spike".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 1 },
        window_seconds: 60,
        actions: vec![ActionRef {
            action_type: ActionType::Webhook,
            config: serde_json::json!({"url": webhook_url}),
        }],
        enabled: true,
    };
    let app = Router::new().route("/v1/triggers/:id", get(handler)).with_state(trigger);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Stands in for the third-party webhook target a `Webhook` action dispatches to.
async fn spawn_stub_webhook_target() -> String {
    async fn handler() -> impl IntoResponse {
        axum::http::StatusCode::OK
    }
    let app = Router::new().route("/webhook", axum::routing::post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/webhook")
}

#[tokio::test]
async fn a_real_event_created_message_results_in_a_dispatched_action_and_an_execution_row() {
    let pool = test_pool().await;
    let publish_channel = test_channel().await;
    let consume_channel = test_channel().await;

    let tenant_id = Uuid::new_v4();
    let trigger_id = Uuid::new_v4();
    let webhook_url = spawn_stub_webhook_target().await;
    let trigger_engine_url = spawn_stub_trigger_engine(trigger_id, tenant_id, webhook_url).await;

    let execution_repository = Arc::new(PostgresExecutionRepository::new(pool));
    let deps = ActionDeps {
        trigger_client: Arc::new(HttpTriggerClient::new(
            reqwest::Client::new(),
            trigger_engine_url,
            "test-internal-secret".to_string(),
        )),
        dispatcher: Arc::new(RoutingActionDispatcher::new(None)),
        execution_repository: execution_repository.clone(),
    };

    // Mirrors main.rs's own exchange declare + a durable queue bind, proving this test observes
    // the exact real exchange the production consumer reads from, not a private test fixture.
    publish_channel
        .exchange_declare(
            EVENT_CREATED_EXCHANGE,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare exchange");
    let queue = consume_channel
        .queue_declare(
            "",
            QueueDeclareOptions { exclusive: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    consume_channel
        .queue_bind(
            queue.name().as_str(),
            EVENT_CREATED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    let mut consumer = consume_channel
        .basic_consume(
            queue.name().as_str(),
            "action-executor-integration-test",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let event = Event {
        id: Uuid::new_v4(),
        tenant_id,
        event_type: "sentiment_spike".to_string(),
        source_connector_ids: vec!["zendesk".to_string()],
        entity_ref: "customer-42".to_string(),
        group_key: "customer-42".to_string(),
        payload: serde_json::json!({"triggered_by": trigger_id.to_string()}),
        occurred_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        status: EventStatus::New,
        record_ids: vec![],
    };
    let payload = serde_json::to_vec(&event).unwrap();
    publish_channel
        .basic_publish(
            EVENT_CREATED_EXCHANGE,
            "",
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default().with_content_type("application/json".into()),
        )
        .await
        .expect("failed to publish event.created")
        .await
        .expect("publish confirm failed");

    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for event.created")
        .expect("consumer stream ended unexpectedly")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("ack failed");
    let received: Event = serde_json::from_slice(&delivery.data).unwrap();
    assert_eq!(received.id, event.id);

    // main.rs's consume loop calls exactly this for every delivery it acks.
    let executed = process_event(&deps, &received).await.expect("process_event failed");
    assert_eq!(executed, 1);

    let executions = execution_repository
        .list_by_event(tenant_id, event.id)
        .await
        .expect("failed to list executions");
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].event_id, event.id);
    assert_eq!(executions[0].trigger_id, trigger_id);
    assert_eq!(executions[0].status, ActionExecutionStatus::Sent);
}
