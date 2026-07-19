//! Integration test against real Postgres (CLAUDE.md §2), proving the local `agents` mirror
//! table (ADR-0020) actually upserts, lists only enabled agents, tracks `last_polled_at`, and
//! deletes correctly. Requires DATABASE_URL.

use agent_scheduler::{AgentRepository, PostgresAgentRepository};
use common::Agent;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
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
    pool
}

fn sample_agent(enabled: bool) -> Agent {
    Agent {
        enabled,
        ..Agent::new(
            Uuid::new_v4(),
            "zendesk",
            "integration-test-agent",
            serde_json::json!({"poll_interval_seconds": 60}),
        )
    }
}

#[tokio::test]
async fn upsert_then_list_enabled_round_trips_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAgentRepository::new(pool);
    let agent = sample_agent(true);

    repo.upsert(agent.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert!(enabled.iter().any(|a| a.agent.id == agent.id && a.last_polled_at.is_none()));

    repo.delete(agent.id).await.unwrap();
}

#[tokio::test]
async fn list_enabled_excludes_disabled_agents_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAgentRepository::new(pool);
    let agent = sample_agent(false);

    repo.upsert(agent.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert!(!enabled.iter().any(|a| a.agent.id == agent.id));

    repo.delete(agent.id).await.unwrap();
}

#[tokio::test]
async fn mark_polled_and_delete_work_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresAgentRepository::new(pool);
    let agent = sample_agent(true);
    repo.upsert(agent.clone()).await.unwrap();

    let now = chrono::Utc::now();
    repo.mark_polled(agent.id, now).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    let found = enabled.iter().find(|a| a.agent.id == agent.id).unwrap();
    assert!(found.last_polled_at.is_some());

    repo.delete(agent.id).await.unwrap();
    let enabled_after_delete = repo.list_enabled().await.unwrap();
    assert!(!enabled_after_delete.iter().any(|a| a.agent.id == agent.id));
}
