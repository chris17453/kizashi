//! Integration test against real Postgres (CLAUDE.md §2). Requires DATABASE_URL.

use auth_service::{
    hash_password, LocalUser, LocalUserRepository, MfaChallengeRepository,
    PostgresLocalUserRepository, PostgresMfaChallengeRepository,
};
use common::Role;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let pool = common::connect_with_schema(&database_url, "auth_service")
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

async fn create_user(repo: &PostgresLocalUserRepository, tenant_id: Uuid) -> LocalUser {
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: format!("user-{}", Uuid::new_v4()),
        password_hash: hash_password("correct-horse-battery-staple").unwrap(),
        role: Role::Operator,
        mfa_secret: None,
        mfa_enabled: false,
    };
    repo.create(user, "test-actor").await.unwrap()
}

#[tokio::test]
async fn set_pending_then_confirm_persists_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let user = create_user(&repo, tenant_id).await;

    repo.set_pending_mfa_secret(user.id, "SECRETBASE32").await.unwrap();
    let pending = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert_eq!(pending.mfa_secret, Some("SECRETBASE32".to_string()));
    assert!(!pending.mfa_enabled);

    repo.confirm_mfa(user.id).await.unwrap();
    let confirmed = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert!(confirmed.mfa_enabled);
}

#[tokio::test]
async fn disable_mfa_clears_the_secret_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let user = create_user(&repo, tenant_id).await;
    repo.set_pending_mfa_secret(user.id, "SECRETBASE32").await.unwrap();
    repo.confirm_mfa(user.id).await.unwrap();

    repo.disable_mfa(user.id).await.unwrap();

    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert_eq!(found.mfa_secret, None);
    assert!(!found.mfa_enabled);
}

#[tokio::test]
async fn find_by_id_is_not_scoped_by_tenant() {
    let pool = test_pool().await;
    let repo = PostgresLocalUserRepository::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let user = create_user(&repo, tenant_id).await;

    let found = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert_eq!(found.id, user.id);
    assert_eq!(found.tenant_id, tenant_id);
}

#[tokio::test]
async fn a_challenge_token_round_trips_and_is_single_use_against_real_postgres() {
    let pool = test_pool().await;
    let repo = PostgresMfaChallengeRepository::new(pool);
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let token = repo.create(user_id, tenant_id).await.unwrap();
    let first = repo.consume(&token).await.unwrap();
    let second = repo.consume(&token).await.unwrap();

    assert_eq!(first, Some((user_id, tenant_id)));
    assert_eq!(second, None, "a challenge token must not be reusable");
}

#[tokio::test]
async fn an_unknown_challenge_token_returns_none() {
    let pool = test_pool().await;
    let repo = PostgresMfaChallengeRepository::new(pool);

    let result = repo.consume("unknown-token").await.unwrap();

    assert_eq!(result, None);
}
