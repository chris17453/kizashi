use super::*;

fn sample_session() -> Session {
    Session {
        bearer_token: "tok".to_string(),
        tenant_id: Uuid::new_v4(),
        username: "alice".to_string(),
        role: common::Role::Admin,
        created_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let store = InMemorySessionStore::default();
    let session = sample_session();

    let session_id = store.create(session.clone()).await;
    let found = store.get(&session_id).await;

    assert_eq!(found, Some(session));
}

#[tokio::test]
async fn get_returns_none_for_unknown_session_id() {
    let store = InMemorySessionStore::default();
    assert_eq!(store.get("unknown").await, None);
}

#[tokio::test]
async fn delete_removes_the_session() {
    let store = InMemorySessionStore::default();
    let session_id = store.create(sample_session()).await;

    store.delete(&session_id).await;

    assert_eq!(store.get(&session_id).await, None);
}

#[tokio::test]
async fn each_created_session_gets_a_distinct_id() {
    let store = InMemorySessionStore::default();
    let id_a = store.create(sample_session()).await;
    let id_b = store.create(sample_session()).await;
    assert_ne!(id_a, id_b);
}

#[tokio::test]
async fn list_for_tenant_only_returns_sessions_for_that_tenant() {
    let store = InMemorySessionStore::default();
    let tenant_a = sample_session();
    let mut tenant_b = sample_session();
    tenant_b.tenant_id = Uuid::new_v4();
    let id_a = store.create(tenant_a.clone()).await;
    store.create(tenant_b).await;

    let listed = store.list_for_tenant(tenant_a.tenant_id).await;

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], (id_a, tenant_a));
}

#[tokio::test]
async fn list_for_tenant_reflects_deletions() {
    let store = InMemorySessionStore::default();
    let session = sample_session();
    let session_id = store.create(session.clone()).await;

    store.delete(&session_id).await;

    assert!(store.list_for_tenant(session.tenant_id).await.is_empty());
}

#[tokio::test]
async fn get_returns_none_once_the_idle_timeout_has_elapsed() {
    let store = InMemorySessionStore::with_idle_timeout(chrono::Duration::milliseconds(1));
    let session_id = store.create(sample_session()).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(store.get(&session_id).await, None);
}

#[tokio::test]
async fn get_before_the_idle_timeout_elapses_still_returns_the_session() {
    let store = InMemorySessionStore::with_idle_timeout(chrono::Duration::minutes(30));
    let session = sample_session();
    let session_id = store.create(session.clone()).await;

    assert_eq!(store.get(&session_id).await, Some(session));
}

#[tokio::test]
async fn activity_slides_the_idle_timeout_forward() {
    // A short-but-not-instant timeout: two gets spaced half the timeout apart should both
    // succeed, because each `get()` refreshes the idle clock -- if it didn't, the second get
    // would still be inside the *original* window and this test wouldn't distinguish sliding
    // from fixed-expiry behavior. What actually proves sliding is the third sleep: it's longer
    // than the timeout measured from creation, but well within it measured from the second get.
    let store = InMemorySessionStore::with_idle_timeout(chrono::Duration::milliseconds(60));
    let session_id = store.create(sample_session()).await;

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(store.get(&session_id).await.is_some(), "still within the original window");

    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    assert!(
        store.get(&session_id).await.is_some(),
        "70ms since creation exceeds the 60ms timeout, but only 40ms since the last get -- \
         sliding renewal should have kept this session alive"
    );
}

#[tokio::test]
async fn list_for_tenant_prunes_expired_sessions() {
    let store = InMemorySessionStore::with_idle_timeout(chrono::Duration::milliseconds(1));
    let session = sample_session();
    store.create(session.clone()).await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(store.list_for_tenant(session.tenant_id).await.is_empty());
}
