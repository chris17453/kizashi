use retention_service::{
    build_router, AppState, HttpRawRecordClient, PostgresAuditLogReader,
    PostgresComplianceHoldRepository, PostgresRetentionPolicyRepository, S3ArchiveStore,
};
use std::sync::Arc;

async fn build_s3_client() -> aws_sdk_s3::Client {
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region));
    if let Ok(endpoint_url) = std::env::var("S3_ENDPOINT_URL") {
        loader = loader.endpoint_url(endpoint_url);
    }
    let shared_config = loader.load().await;
    let s3_config = aws_sdk_s3::config::Builder::from(&shared_config)
        // Self-hosted S3-compatible targets (MinIO) need path-style addressing; real AWS S3
        // tolerates it too, so it's safe to always set (ADR-0011).
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(s3_config)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let ingestion_service_url =
        std::env::var("INGESTION_SERVICE_URL").expect("INGESTION_SERVICE_URL must be set");
    let bucket = std::env::var("AWS_S3_BUCKET").expect("AWS_S3_BUCKET must be set");
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");

    let pool = common::connect_with_schema(&database_url, "retention_service")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let s3_client = build_s3_client().await;
    let archive_store = S3ArchiveStore::new(s3_client, bucket);
    archive_store.ensure_bucket().await.expect("failed to ensure archive bucket exists");

    let state = AppState {
        policy_repository: Arc::new(PostgresRetentionPolicyRepository::new(pool.clone())),
        audit_reader: Arc::new(PostgresAuditLogReader::new(pool.clone())),
        record_client: Arc::new(HttpRawRecordClient::new(
            reqwest::Client::new(),
            ingestion_service_url,
        )),
        archive_store: Arc::new(archive_store),
        internal_secret,
        hold_repository: Some(Arc::new(PostgresComplianceHoldRepository::new(pool))),
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "retention-service listening");
    axum::serve(listener, build_router(state)).await.expect("server error");
}
