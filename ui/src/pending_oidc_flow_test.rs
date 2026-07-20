use super::*;

fn sample_flow() -> PendingOidcFlow {
    PendingOidcFlow {
        provider: "entra".to_string(),
        csrf_token: "csrf-abc".to_string(),
        code_verifier: "verifier-abc".to_string(),
        tenant_name: "acme".to_string(),
    }
}

#[tokio::test]
async fn stores_and_retrieves_a_pending_flow() {
    let store = InMemoryPendingOidcFlowStore::default();
    let flow = sample_flow();

    let id = store.create(flow.clone()).await;
    let found = store.get(&id).await;

    assert_eq!(found, Some(flow));
}

#[tokio::test]
async fn take_removes_the_flow_so_it_cannot_be_replayed() {
    let store = InMemoryPendingOidcFlowStore::default();
    let flow = sample_flow();
    let id = store.create(flow.clone()).await;

    let first = store.take(&id).await;
    let second = store.take(&id).await;

    assert_eq!(first, Some(flow));
    assert_eq!(second, None);
}

#[tokio::test]
async fn get_returns_none_for_an_unknown_id() {
    let store = InMemoryPendingOidcFlowStore::default();
    assert_eq!(store.get("nonexistent").await, None);
}
