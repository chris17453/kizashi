//! End-to-end tenant-isolation test against the REAL proxy path (CLAUDE.md §5: "every query
//! path must be tested for tenant isolation, not just implemented correctly by inspection").
//! Query Gateway is spec §6's designated single tenant-enforcement point for all UI/dashboard
//! traffic — this spins up a real `query-gateway` server, a real `dashboard-api` server, real
//! Postgres (for token resolution), and real ClickHouse (for event storage), and proves that a
//! session token minted for tenant A cannot retrieve an event that belongs to tenant B through
//! the actual HTTP proxy path — not a mocked `TokenStore`, not a stubbed upstream.
//!
//! Requires DATABASE_URL and CLICKHOUSE_URL.

use common::Role;
use dashboard_api::{ClickHouseEventQueryRepository, DashboardState};
use query_gateway::{build_router, GatewayState, PostgresTokenStore, TokenStore};
use std::sync::Arc;
use uuid::Uuid;

async fn query_gateway_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "query_gateway")
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

async fn ensure_clickhouse_schema(client: &reqwest::Client, base_url: &str) {
    let ddl = r#"
        CREATE TABLE IF NOT EXISTS events (
            id UUID,
            tenant_id UUID,
            event_type String,
            source_connector_ids Array(String),
            entity_ref String,
            group_key String,
            payload String,
            occurred_at DateTime64(3),
            created_at DateTime64(3),
            status String,
            record_ids Array(UUID)
        ) ENGINE = MergeTree() ORDER BY (tenant_id, occurred_at)
    "#;
    let response = client
        .post(base_url)
        .body(ddl.to_string())
        .send()
        .await
        .expect("failed to reach clickhouse");
    assert!(response.status().is_success(), "failed to ensure schema: {:?}", response.text().await);
}

async fn insert_event(client: &reqwest::Client, base_url: &str, id: Uuid, tenant_id: Uuid) {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let row = serde_json::json!({
        "id": id,
        "tenant_id": tenant_id,
        "event_type": "sentiment",
        "source_connector_ids": ["zendesk"],
        "entity_ref": "cust-isolation-test",
        "group_key": "cust-isolation-test",
        "payload": serde_json::to_string(&serde_json::json!({"value": -0.9})).unwrap(),
        "occurred_at": now,
        "created_at": now,
        "status": "new",
        "record_ids": [],
    });
    let response = client
        .post(base_url)
        .query(&[("query", "INSERT INTO events FORMAT JSONEachRow")])
        .body(serde_json::to_vec(&row).unwrap())
        .send()
        .await
        .expect("failed to reach clickhouse");
    assert!(response.status().is_success(), "insert failed: {:?}", response.text().await);
}

async fn spawn_router(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn a_session_token_for_one_tenant_cannot_retrieve_another_tenants_event_through_the_real_proxy(
) {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let clickhouse_base_url = format!("{clickhouse_url}/");
    let http_client = reqwest::Client::new();
    ensure_clickhouse_schema(&http_client, &clickhouse_base_url).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let event_owned_by_tenant_a = Uuid::new_v4();
    insert_event(&http_client, &clickhouse_base_url, event_owned_by_tenant_a, tenant_a).await;

    // Real dashboard-api, backed by real ClickHouse.
    let dashboard_state = DashboardState {
        event_query_repository: Arc::new(ClickHouseEventQueryRepository::new(
            reqwest::Client::new(),
            clickhouse_base_url,
        )),
    };
    let dashboard_api_url = spawn_router(dashboard_api::build_router(dashboard_state)).await;

    // Real query-gateway, backed by a real Postgres-backed TokenStore, pointed at the real
    // dashboard-api instance above.
    let pool = query_gateway_pool().await;
    let token_store = Arc::new(PostgresTokenStore::new(pool));
    let tenant_a_token =
        token_store.mint_token(tenant_a, Role::Viewer, "isolation-test-a").await.unwrap();
    let tenant_b_token =
        token_store.mint_token(tenant_b, Role::Viewer, "isolation-test-b").await.unwrap();

    let gateway_state = GatewayState {
        token_store,
        http_client: reqwest::Client::new(),
        dashboard_api_url,
        internal_secret: "unused-in-this-test".to_string(),
    };
    let gateway_url = spawn_router(build_router(gateway_state)).await;

    // Tenant A's own token retrieves its own event through the real proxy.
    let as_tenant_a = reqwest::Client::new()
        .get(format!("{gateway_url}/v1/events/{event_owned_by_tenant_a}"))
        .bearer_auth(&tenant_a_token)
        .send()
        .await
        .unwrap();
    assert_eq!(as_tenant_a.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = as_tenant_a.json().await.unwrap();
    assert_eq!(body["id"], event_owned_by_tenant_a.to_string());

    // Tenant B's token, requesting the SAME event id, gets nothing back — the real proxy
    // resolved tenant B's own identity from its token and forwarded that (not any
    // client-supplied value), and the real dashboard-api's real ClickHouse query is scoped to
    // that tenant, so tenant A's row is invisible.
    let as_tenant_b = reqwest::Client::new()
        .get(format!("{gateway_url}/v1/events/{event_owned_by_tenant_a}"))
        .bearer_auth(&tenant_b_token)
        .send()
        .await
        .unwrap();
    assert_eq!(as_tenant_b.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn listing_events_through_the_real_proxy_never_returns_another_tenants_rows() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let clickhouse_base_url = format!("{clickhouse_url}/");
    let http_client = reqwest::Client::new();
    ensure_clickhouse_schema(&http_client, &clickhouse_base_url).await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    insert_event(&http_client, &clickhouse_base_url, Uuid::new_v4(), tenant_a).await;
    insert_event(&http_client, &clickhouse_base_url, Uuid::new_v4(), tenant_b).await;

    let dashboard_state = DashboardState {
        event_query_repository: Arc::new(ClickHouseEventQueryRepository::new(
            reqwest::Client::new(),
            clickhouse_base_url,
        )),
    };
    let dashboard_api_url = spawn_router(dashboard_api::build_router(dashboard_state)).await;

    let pool = query_gateway_pool().await;
    let token_store = Arc::new(PostgresTokenStore::new(pool));
    let tenant_a_token =
        token_store.mint_token(tenant_a, Role::Viewer, "isolation-list-test-a").await.unwrap();

    let gateway_state = GatewayState {
        token_store,
        http_client: reqwest::Client::new(),
        dashboard_api_url,
        internal_secret: "unused-in-this-test".to_string(),
    };
    let gateway_url = spawn_router(build_router(gateway_state)).await;

    let response = reqwest::Client::new()
        .get(format!("{gateway_url}/v1/events"))
        .bearer_auth(&tenant_a_token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    let events = body["events"].as_array().expect("expected an `events` array");
    assert!(
        !events.is_empty(),
        "expected at least tenant A's own event in the listing, got: {events:?}"
    );
    assert!(
        events.iter().all(|e| e["tenant_id"] == tenant_a.to_string()),
        "listing through the real proxy leaked a row not owned by the calling tenant: {events:?}"
    );
}

#[tokio::test]
async fn an_invalid_bearer_token_is_rejected_by_the_real_proxy_before_reaching_dashboard_api() {
    let clickhouse_url =
        std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL must be set to run this test");
    let clickhouse_base_url = format!("{clickhouse_url}/");

    let dashboard_state = DashboardState {
        event_query_repository: Arc::new(ClickHouseEventQueryRepository::new(
            reqwest::Client::new(),
            clickhouse_base_url,
        )),
    };
    let dashboard_api_url = spawn_router(dashboard_api::build_router(dashboard_state)).await;

    let pool = query_gateway_pool().await;
    let token_store = Arc::new(PostgresTokenStore::new(pool));
    let gateway_state = GatewayState {
        token_store,
        http_client: reqwest::Client::new(),
        dashboard_api_url,
        internal_secret: "unused-in-this-test".to_string(),
    };
    let gateway_url = spawn_router(build_router(gateway_state)).await;

    let response = reqwest::Client::new()
        .get(format!("{gateway_url}/v1/events/{}", Uuid::new_v4()))
        .bearer_auth("this-token-was-never-minted")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}
