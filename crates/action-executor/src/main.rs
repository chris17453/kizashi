use action_executor::{
    execution_router, health_router, process_event, ActionDeps, ExecutionState,
    HttpActionDispatcher, HttpTriggerClient, PostgresExecutionRepository, EVENT_CREATED_EXCHANGE,
};
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use std::sync::Arc;

const QUEUE_NAME: &str = "action-executor.event.created";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let trigger_engine_url =
        std::env::var("TRIGGER_ENGINE_URL").expect("TRIGGER_ENGINE_URL must be set");
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

    let execution_repository = Arc::new(PostgresExecutionRepository::new(pool));
    let deps = ActionDeps {
        trigger_client: Arc::new(HttpTriggerClient::new(
            reqwest::Client::new(),
            trigger_engine_url,
        )),
        dispatcher: Arc::new(HttpActionDispatcher::new(reqwest::Client::new())),
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
        .queue_bind(
            QUEUE_NAME,
            EVENT_CREATED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "action-executor http listening");
    let http_router =
        health_router().merge(execution_router(ExecutionState { execution_repository }));
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
                tracing::error!(event_id = %event.id, error = %e, "processing failed, requeueing");
                let _ =
                    delivery.nack(BasicNackOptions { requeue: true, ..Default::default() }).await;
            }
        }
    }
}
