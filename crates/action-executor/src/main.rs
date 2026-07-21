use action_executor::{
    dead_letter_router, execution_router, health_router, process_event, retry_count,
    should_dead_letter, with_incremented_retry_count, ActionDeps, DeadLetterState, ExecutionState,
    HttpTriggerClient, PostgresExecutionRepository, RabbitMqDeadLetterManager,
    RoutingActionDispatcher, EVENT_CREATED_EXCHANGE,
};
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, QueueBindOptions,
    QueueDeclareOptions,
};
use lapin::types::FieldTable;
use std::sync::Arc;

const QUEUE_NAME: &str = "action-executor.event.created";
const DEAD_LETTER_QUEUE_NAME: &str = "action-executor.event.created.dead";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let trigger_engine_url =
        std::env::var("TRIGGER_ENGINE_URL").expect("TRIGGER_ENGINE_URL must be set");
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

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

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let consume_channel = connection.create_channel().await.expect("failed to open channel");
    let dead_letter_channel = connection.create_channel().await.expect("failed to open channel");

    // ADR-0021: opt-in — set EGRESS_PROXY_URL to route every action webhook dispatch (an
    // external, often customer-controlled endpoint) through Egress Gateway's audit log/
    // allowlist; unset means today's exact behavior. trigger_client below stays unproxied —
    // it calls trigger-engine, a Kizashi-owned service, not an external one.
    let egress_proxy_url = std::env::var("EGRESS_PROXY_URL").ok();

    let execution_repository = Arc::new(PostgresExecutionRepository::new(pool));
    let deps = ActionDeps {
        trigger_client: Arc::new(HttpTriggerClient::new(
            reqwest::Client::new(),
            trigger_engine_url,
            internal_secret.clone(),
        )),
        dispatcher: Arc::new(RoutingActionDispatcher::new(egress_proxy_url)),
        execution_repository: execution_repository.clone(),
    };

    consume_channel
        .queue_declare(
            QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    consume_channel
        .queue_declare(
            DEAD_LETTER_QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare dead-letter queue");
    consume_channel
        .queue_bind(
            QUEUE_NAME,
            EVENT_CREATED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let dead_letter_state = DeadLetterState {
        dead_letter_manager: Arc::new(RabbitMqDeadLetterManager::new(
            dead_letter_channel,
            DEAD_LETTER_QUEUE_NAME.to_string(),
            QUEUE_NAME.to_string(),
        )),
        internal_secret,
    };
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "action-executor http listening");
    let http_router = health_router()
        .merge(execution_router(ExecutionState { execution_repository }))
        .merge(dead_letter_router(dead_letter_state));
    tokio::spawn(async move {
        axum::serve(listener, http_router).await.expect("http server error");
    });

    let mut consumer = consume_channel
        .basic_consume(
            QUEUE_NAME,
            "action-executor",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    tracing::info!("action-executor consuming event.created");
    while let Some(delivery) = consumer.next().await {
        let delivery = match delivery {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "consumer delivery error");
                continue;
            }
        };

        let event: common::Event = match serde_json::from_slice(&delivery.data) {
            Ok(e) => e,
            Err(e) => {
                tracing::error!(error = %e, "failed to deserialize event.created message, dropping");
                let _ = delivery.ack(BasicAckOptions::default()).await;
                continue;
            }
        };

        match process_event(&deps, &event).await {
            Ok(executed) => {
                tracing::info!(event_id = %event.id, actions_executed = executed, "actions executed");
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
            Err(e) => {
                let headers = delivery.properties.headers().as_ref();
                if should_dead_letter(headers) {
                    tracing::error!(
                        event_id = %event.id,
                        error = %e,
                        retries = retry_count(headers),
                        "message exceeded max retries, dead-lettering"
                    );
                    let _ = consume_channel
                        .basic_publish(
                            "",
                            DEAD_LETTER_QUEUE_NAME,
                            BasicPublishOptions::default(),
                            &delivery.data,
                            delivery.properties.clone(),
                        )
                        .await;
                } else {
                    tracing::error!(event_id = %event.id, error = %e, "processing failed, requeueing");
                    let next_headers = with_incremented_retry_count(headers);
                    let _ = consume_channel
                        .basic_publish(
                            "",
                            QUEUE_NAME,
                            BasicPublishOptions::default(),
                            &delivery.data,
                            delivery.properties.clone().with_headers(next_headers),
                        )
                        .await;
                }
                let _ = delivery.ack(BasicAckOptions::default()).await;
            }
        }
    }
}
