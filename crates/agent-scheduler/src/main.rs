use agent_scheduler::{
    health_router, AgentRepository, DockerInvoker, Invoker, PostgresAgentRepository,
    AGENT_CHANGED_EXCHANGE,
};
use common::AgentChangeEvent;
use futures_util::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, ExchangeDeclareOptions,
    QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::ExchangeKind;
use std::sync::Arc;
use std::time::Duration;

const QUEUE_NAME: &str = "agent-scheduler.agent.changed";
const DEFAULT_POLL_INTERVAL_SECONDS: i64 = 300;

fn tick_interval() -> Duration {
    let secs =
        std::env::var("SCHEDULER_TICK_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(15);
    Duration::from_secs(secs)
}

fn poll_interval_seconds(config: &serde_json::Value) -> i64 {
    config
        .get("poll_interval_seconds")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rabbitmq_url = std::env::var("RABBITMQ_URL").expect("RABBITMQ_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let docker_image_prefix =
        std::env::var("DOCKER_IMAGE_PREFIX").unwrap_or_else(|_| "kizashi".to_string());
    let docker_network =
        std::env::var("DOCKER_NETWORK").unwrap_or_else(|_| "kizashi_default".to_string());
    let ingestion_gateway_url =
        std::env::var("INGESTION_GATEWAY_URL").expect("INGESTION_GATEWAY_URL must be set");
    let ingestion_gateway_api_key =
        std::env::var("INGESTION_GATEWAY_API_KEY").expect("INGESTION_GATEWAY_API_KEY must be set");
    if ingestion_gateway_api_key.is_empty() {
        // Not a startup failure — docker-compose sets this to an empty string by default
        // (ADR-0020's v1 platform-wide-key simplification) rather than refusing to start the
        // whole stack over one unconfigured key. Every scheduled connector invocation will
        // fail to authenticate until a real key is set; a loud warning here beats silently
        // discovering it later in per-invocation error logs.
        tracing::warn!(
            "INGESTION_GATEWAY_API_KEY is empty — every scheduled connector invocation will \
             fail to authenticate until AGENT_SCHEDULER_INGESTION_GATEWAY_API_KEY is set to a \
             real key (create one via the Console UI's API Keys page)"
        );
    }

    let pool = common::connect_with_schema(&database_url, "agent_scheduler")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let agent_repository: Arc<dyn AgentRepository> = Arc::new(PostgresAgentRepository::new(pool));
    let invoker: Arc<dyn Invoker> = Arc::new(DockerInvoker::new(
        docker_image_prefix,
        docker_network,
        ingestion_gateway_url,
        ingestion_gateway_api_key,
    ));

    let connection =
        lapin::Connection::connect(&rabbitmq_url, lapin::ConnectionProperties::default())
            .await
            .expect("failed to connect to rabbitmq");
    let channel = connection.create_channel().await.expect("failed to open channel");
    channel
        .exchange_declare(
            AGENT_CHANGED_EXCHANGE,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare agent.changed exchange");
    channel
        .queue_declare(
            QUEUE_NAME,
            QueueDeclareOptions { durable: true, ..Default::default() },
            FieldTable::default(),
        )
        .await
        .expect("failed to declare queue");
    channel
        .queue_bind(
            QUEUE_NAME,
            AGENT_CHANGED_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to bind queue");

    let mut consumer = channel
        .basic_consume(
            QUEUE_NAME,
            "agent-scheduler",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("failed to start consumer");

    let sync_repository = agent_repository.clone();
    tokio::spawn(async move {
        tracing::info!("agent-scheduler consuming agent.changed");
        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(error = %e, "agent.changed consumer delivery error");
                    continue;
                }
            };

            let event: AgentChangeEvent = match serde_json::from_slice(&delivery.data) {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!(error = %e, "failed to deserialize agent.changed message, dropping");
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                    continue;
                }
            };

            let result = match &event {
                AgentChangeEvent::Upserted(agent) => sync_repository.upsert(agent.clone()).await,
                AgentChangeEvent::Deleted { id, .. } => sync_repository.delete(*id).await,
            };

            match result {
                Ok(()) => {
                    tracing::info!("synced agent.changed");
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to sync agent, requeueing");
                    let _ = delivery
                        .nack(BasicNackOptions { requeue: true, ..Default::default() })
                        .await;
                }
            }
        }
    });

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tick_interval());
        loop {
            ticker.tick().await;
            let enabled = match agent_repository.list_enabled().await {
                Ok(agents) => agents,
                Err(e) => {
                    tracing::error!(error = %e, "failed to list enabled agents");
                    continue;
                }
            };

            let now = chrono::Utc::now();
            for stored in enabled {
                let interval = poll_interval_seconds(&stored.agent.config);
                let due = match stored.last_polled_at {
                    None => true,
                    Some(last) => (now - last).num_seconds() >= interval,
                };
                if !due {
                    continue;
                }

                tracing::info!(agent_id = %stored.agent.id, name = %stored.agent.name, "invoking due agent");
                if let Err(e) = invoker.invoke(&stored.agent).await {
                    tracing::error!(agent_id = %stored.agent.id, error = %e, "agent invocation failed");
                }
                if let Err(e) = agent_repository.mark_polled(stored.agent.id, now).await {
                    tracing::error!(agent_id = %stored.agent.id, error = %e, "failed to record poll timestamp");
                }
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "agent-scheduler listening");
    axum::serve(listener, health_router()).await.expect("server error");
}
