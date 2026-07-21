//! The full-pipeline e2e test CLAUDE.md §2 has required since day one: a `RawRecord` posted
//! through the real chain ingestion -> normalization -> analysis -> trigger -> action, against
//! real Postgres, real RabbitMQ, and real ClickHouse (no mocks). Each stage's own crate is
//! reused as a library, chained via real message-bus round trips (publish with the upstream
//! stage's own publisher, consume with a queue bound to the real exchange, then call that
//! stage's own processing function against real infra) — the same shape every per-service
//! integration test in this codebase already uses, just run back-to-back instead of in
//! isolation. Requires DATABASE_URL, RABBITMQ_URL, CLICKHOUSE_URL.
//!
//! Two seams are deliberately stubbed rather than run for real, consistent with how the
//! per-service tests already scope themselves:
//! - The AI/ML analysis call (`analysis-service`'s `AnalysisClient`) is an in-process stub
//!   returning a fixed `{"sentiment_spike": 1}` result — no real Azure AI Foundry endpoint
//!   exists in this environment, and analysis-service's own tests already cover the real HTTP
//!   client behavior in isolation.
//! - Action Executor's Trigger Engine lookup (`TriggerClient`) is a stub HTTP server returning
//!   the fixed trigger this test creates, exactly like `action-executor`'s own
//!   `rabbitmq_integration_test.rs` already does — Trigger Engine's real HTTP API is covered by
//!   its own tests.

use action_executor::{
    process_event, ActionDeps, ExecutionRepository, HttpTriggerClient, PostgresExecutionRepository,
    RoutingActionDispatcher, EVENT_CREATED_EXCHANGE,
};
use analysis_service::{
    process_batch, AnalysisClient, AnalysisDeps, AnalysisError, PostgresAnalysisConfigRepository,
    RabbitMqEventPublisher as AnalysisEventPublisher, RECORD_ANALYZED_EXCHANGE,
    RECORD_NORMALIZED_EXCHANGE as ANALYSIS_RECORD_NORMALIZED_EXCHANGE,
};
use async_trait::async_trait;
use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::{get, patch};
use axum::{Json, Router};
use common::{
    ActionRef, ActionType, AnalyzedRecord, RawRecord, SourceType, TriggerCondition,
    TriggerDefinition,
};
use futures_util::StreamExt;
use ingestion_service::{
    EventPublisher as IngestionEventPublisher,
    RabbitMqEventPublisher as IngestionRabbitMqEventPublisher, RECORD_INGESTED_EXCHANGE,
};
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use normalization_service::{
    process_normalization, HttpRecordClient, NormalizationDeps, PostgresFingerprintRepository,
    PostgresMappingRepository, RabbitMqEventPublisher as NormalizationEventPublisher,
};
use std::sync::Arc;
use trigger_engine::{
    process_analyzed_record, ClickHouseEventStore, PostgresSignalRepository,
    PostgresTriggerRepository, RabbitMqEventPublisher as TriggerEventPublisher, TriggerDeps,
    TriggerRepository,
};
use uuid::Uuid;

async fn test_pool(schema: &str) -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    common::connect_with_schema(&database_url, schema)
        .await
        .unwrap_or_else(|e| panic!("failed to connect to postgres schema {schema}: {e}"))
}

async fn run_migrations(pool: &sqlx::PgPool, manifest_dir: &str) {
    let migrations_dir = std::path::Path::new(manifest_dir).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(pool)
        .await
        .expect("failed to run migrations");
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

/// Binds a fresh exclusive queue to `exchange` and returns the ready-to-consume consumer.
/// `exchange` must already exist (declared by its own stage's publisher) before this runs, same
/// ordering constraint `scripts/run-local.sh` documents for the real services.
async fn bind_consumer(channel: &lapin::Channel, exchange: &str, tag: &str) -> lapin::Consumer {
    let queue = channel
        .queue_declare(
            "",
            QueueDeclareOptions { exclusive: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    channel
        .queue_bind(
            queue.name().as_str(),
            exchange,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");
    channel
        .basic_consume(
            queue.name().as_str(),
            tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer")
}

async fn recv<T: serde::de::DeserializeOwned>(consumer: &mut lapin::Consumer) -> T {
    let delivery = tokio::time::timeout(std::time::Duration::from_secs(5), consumer.next())
        .await
        .expect("timed out waiting for message")
        .expect("consumer stream ended unexpectedly")
        .expect("delivery error");
    delivery.ack(BasicAckOptions::default()).await.expect("ack failed");
    serde_json::from_slice(&delivery.data).expect("failed to deserialize message")
}

/// Stands in for the real Azure AI Foundry/ML endpoint (no such endpoint exists in this
/// environment) — always returns a candidate that trips `sentiment_spike`, the same key
/// `trigger_engine::classify::candidates` turns into a `Candidate { event_type:
/// "sentiment_spike", .. }` (ADR-0006: every top-level numeric key in the analysis output
/// becomes a candidate event named after that key).
struct StubAnalysisClient;

#[async_trait]
impl AnalysisClient for StubAnalysisClient {
    async fn analyze_batch(
        &self,
        _tenant_id: Uuid,
        records: &[RawRecord],
        _prompt: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, AnalysisError> {
        Ok(records.iter().map(|_| serde_json::json!({"sentiment_spike": 1})).collect())
    }
}

/// Stands in for Ingestion Service's own `PATCH /v1/records/:id/normalized`, same as
/// `normalization_integration_test.rs`'s own stub — this test only cares that Normalization
/// Service calls it and then publishes, not Ingestion Service's own persistence behavior.
async fn spawn_stub_ingestion_service() -> String {
    async fn handler(
        Path(_id): Path<Uuid>,
        Json(_body): Json<serde_json::Value>,
    ) -> impl IntoResponse {
        axum::http::StatusCode::OK
    }
    let app = Router::new().route("/v1/records/:id/normalized", patch(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Stands in for Trigger Engine's `GET /v1/triggers/:id`, same as `action-executor`'s own
/// `rabbitmq_integration_test.rs` — returns the fixed trigger this test creates.
async fn spawn_stub_trigger_engine(trigger: TriggerDefinition) -> String {
    async fn handler(
        Path(_id): Path<Uuid>,
        axum::extract::State(trigger): axum::extract::State<TriggerDefinition>,
    ) -> impl IntoResponse {
        Json(trigger)
    }
    let app = Router::new().route("/v1/triggers/:id", get(handler)).with_state(trigger);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Stands in for the third-party webhook target the firing trigger's action dispatches to.
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
async fn a_raw_record_flows_all_the_way_from_ingestion_to_a_dispatched_action() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");

    // No Postgres pool for ingestion-service itself: this test drives the chain by publishing
    // record.ingested directly (via ingestion-service's own real publisher), the same seam
    // `POST /v1/records` itself calls after persisting -- Ingestion Service's own persistence
    // behavior is covered by its own tests, not duplicated here.
    let normalization_pool = test_pool("normalization_service").await;
    run_migrations(
        &normalization_pool,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../normalization-service"),
    )
    .await;
    let analysis_pool = test_pool("analysis_service").await;
    run_migrations(&analysis_pool, concat!(env!("CARGO_MANIFEST_DIR"), "/../analysis-service"))
        .await;
    let trigger_pool = test_pool("trigger_engine").await;
    run_migrations(&trigger_pool, concat!(env!("CARGO_MANIFEST_DIR"), "/../trigger-engine")).await;
    let action_pool = test_pool("action_executor").await;
    run_migrations(&action_pool, concat!(env!("CARGO_MANIFEST_DIR"), "/../action-executor")).await;

    let tenant_id = Uuid::new_v4();
    let trigger_id = Uuid::new_v4();

    // --- Fixtures: a normalization mapping and an enabled trigger, inserted directly via each
    // stage's own repository -- the same "insert the fixture row this stage's repository reads"
    // convention every per-service integration test in this codebase already uses, rather than
    // going through config-admin-service's HTTP API + change-event propagation (that
    // propagation path is covered separately by config-admin-service's own tests).
    let mut field_map = std::collections::BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    sqlx::query(
        "INSERT INTO normalization_mappings (id, tenant_id, source_type, field_map, version) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind("ticket")
    .bind(serde_json::to_value(&field_map).unwrap())
    .bind(1)
    .execute(&normalization_pool)
    .await
    .expect("failed to insert normalization mapping fixture");

    let webhook_url = spawn_stub_webhook_target().await;
    let trigger = TriggerDefinition {
        id: trigger_id,
        tenant_id,
        name: "e2e-full-pipeline-trigger".to_string(),
        event_type_match: "sentiment_spike".to_string(),
        condition: TriggerCondition::CountOverWindow { count: 1 },
        window_seconds: 60,
        actions: vec![ActionRef {
            action_type: ActionType::Webhook,
            config: serde_json::json!({"url": webhook_url}),
        }],
        enabled: true,
    };
    PostgresTriggerRepository::new(trigger_pool.clone())
        .upsert(trigger.clone())
        .await
        .expect("failed to insert trigger fixture");

    // --- Stage wiring: each stage's real publisher (declares its own exchange, matching
    // main.rs's ordering constraint) and a consumer bound to it, so this test observes the
    // exact exchanges the real services publish/consume from, not private test fixtures.
    let ingestion_publish_channel = test_channel().await;
    let normalized_consume_channel = test_channel().await;
    let normalization_publish_channel = test_channel().await;
    let analyzed_consume_channel = test_channel().await;
    let analysis_publish_channel = test_channel().await;
    let event_consume_channel = test_channel().await;
    let trigger_publish_channel = test_channel().await;
    let action_consume_channel = test_channel().await;

    let ingestion_publisher = IngestionRabbitMqEventPublisher::new(ingestion_publish_channel)
        .await
        .expect("failed to declare record.ingested exchange");
    let mut normalized_consumer =
        bind_consumer(&normalized_consume_channel, RECORD_INGESTED_EXCHANGE, "e2e-normalization")
            .await;

    let stub_ingestion_service_url = spawn_stub_ingestion_service().await;
    let normalization_publisher = NormalizationEventPublisher::new(normalization_publish_channel)
        .await
        .expect("failed to declare record.normalized exchange");
    let normalization_deps = NormalizationDeps {
        mapping_repository: Arc::new(PostgresMappingRepository::new(normalization_pool.clone())),
        record_client: Arc::new(HttpRecordClient::new(
            reqwest::Client::new(),
            stub_ingestion_service_url,
        )),
        publisher: Arc::new(normalization_publisher),
        fingerprint_repository: Arc::new(PostgresFingerprintRepository::new(normalization_pool)),
    };
    let mut analyzed_input_consumer = bind_consumer(
        &analyzed_consume_channel,
        ANALYSIS_RECORD_NORMALIZED_EXCHANGE,
        "e2e-analysis",
    )
    .await;

    let analysis_publisher = AnalysisEventPublisher::new(analysis_publish_channel)
        .await
        .expect("failed to declare record.analyzed exchange");
    let analysis_deps = AnalysisDeps {
        analysis_client: Arc::new(StubAnalysisClient),
        publisher: Arc::new(analysis_publisher),
        analysis_config_repository: Arc::new(PostgresAnalysisConfigRepository::new(analysis_pool)),
        http_client: reqwest::Client::new(),
        openai_compatible_concurrency: 4,
    };
    let mut event_input_consumer =
        bind_consumer(&event_consume_channel, RECORD_ANALYZED_EXCHANGE, "e2e-trigger-engine").await;

    let trigger_publisher = TriggerEventPublisher::new(trigger_publish_channel)
        .await
        .expect("failed to declare event.created exchange");
    let trigger_deps = TriggerDeps {
        trigger_repository: Arc::new(PostgresTriggerRepository::new(trigger_pool.clone())),
        signal_repository: Arc::new(PostgresSignalRepository::new(trigger_pool)),
        event_store: Arc::new(ClickHouseEventStore::new(reqwest::Client::new(), clickhouse_url)),
        publisher: Arc::new(trigger_publisher),
    };
    let mut action_input_consumer =
        bind_consumer(&action_consume_channel, EVENT_CREATED_EXCHANGE, "e2e-action-executor").await;

    let stub_trigger_engine_url = spawn_stub_trigger_engine(trigger).await;
    let execution_repository = Arc::new(PostgresExecutionRepository::new(action_pool));
    let action_deps = ActionDeps {
        trigger_client: Arc::new(HttpTriggerClient::new(
            reqwest::Client::new(),
            stub_trigger_engine_url,
            "e2e-test-internal-secret".to_string(),
        )),
        dispatcher: Arc::new(RoutingActionDispatcher::new(None)),
        execution_repository: execution_repository.clone(),
    };

    // --- Drive the chain, one real hop at a time ---
    let record = RawRecord::new(
        "zendesk",
        SourceType::Ticket,
        tenant_id,
        serde_json::json!({"description": "the customer is furious and threatening to cancel"}),
    );
    ingestion_publisher
        .publish_record_ingested(&record)
        .await
        .expect("failed to publish record.ingested");

    let ingested: RawRecord = recv(&mut normalized_consumer).await;
    assert_eq!(ingested.id, record.id);
    let outcome =
        process_normalization(&normalization_deps, &ingested).await.expect("normalization failed");
    assert_eq!(outcome, normalization_service::ProcessOutcome::Normalized);

    let normalized: RawRecord = recv(&mut analyzed_input_consumer).await;
    assert_eq!(normalized.id, record.id);
    assert!(normalized.normalized_payload.is_some());
    let published =
        process_batch(&analysis_deps, tenant_id, vec![normalized]).await.expect("analysis failed");
    assert_eq!(published, 1);

    let analyzed: AnalyzedRecord = recv(&mut event_input_consumer).await;
    assert_eq!(analyzed.record.id, record.id);
    assert_eq!(analyzed.analysis, serde_json::json!({"sentiment_spike": 1}));
    let events_created =
        process_analyzed_record(&trigger_deps, &analyzed).await.expect("trigger evaluation failed");
    assert_eq!(
        events_created, 1,
        "the CountOverWindow{{count: 1}} trigger should fire on the first matching signal"
    );

    let event: common::Event = recv(&mut action_input_consumer).await;
    assert_eq!(event.tenant_id, tenant_id);
    assert_eq!(event.event_type, "sentiment_spike");
    let actions_executed =
        process_event(&action_deps, &event).await.expect("action processing failed");
    assert_eq!(actions_executed, 1);

    let executions = execution_repository
        .list_by_event(tenant_id, event.id)
        .await
        .expect("failed to list executions");
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].trigger_id, trigger_id);
    assert_eq!(executions[0].status, common::ActionExecutionStatus::Sent);
}
