use super::*;

fn sample_session() -> Session {
    Session {
        bearer_token: "tok".to_string(),
        tenant_id: Uuid::new_v4(),
        username: "alice".to_string(),
        role: common::Role::Admin,
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
