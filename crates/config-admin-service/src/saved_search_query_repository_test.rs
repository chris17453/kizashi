use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySavedSearchQueryRepository {
    pub queries: Mutex<Vec<SavedSearchQuery>>,
}

#[async_trait]
impl SavedSearchQueryRepository for InMemorySavedSearchQueryRepository {
    async fn create(
        &self,
        query: SavedSearchQuery,
    ) -> Result<SavedSearchQuery, SavedSearchQueryRepositoryError> {
        self.queries.lock().unwrap().push(query.clone());
        Ok(query)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueryRepositoryError> {
        Ok(self
            .queries
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), SavedSearchQueryRepositoryError> {
        let mut queries = self.queries.lock().unwrap();
        let before_len = queries.len();
        queries.retain(|q| !(q.id == id && q.tenant_id == tenant_id));
        if queries.len() == before_len {
            return Err(SavedSearchQueryRepositoryError::NotFound(id));
        }
        Ok(())
    }
}

pub struct FailingSavedSearchQueryRepository;

#[async_trait]
impl SavedSearchQueryRepository for FailingSavedSearchQueryRepository {
    async fn create(
        &self,
        _query: SavedSearchQuery,
    ) -> Result<SavedSearchQuery, SavedSearchQueryRepositoryError> {
        Err(SavedSearchQueryRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueryRepositoryError> {
        Err(SavedSearchQueryRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn delete(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<(), SavedSearchQueryRepositoryError> {
        Err(SavedSearchQueryRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_query(tenant_id: Uuid) -> SavedSearchQuery {
    SavedSearchQuery::new(tenant_id, "urgent tickets", serde_json::json!({"q": "urgent"}))
}

#[tokio::test]
async fn create_adds_a_query_that_list_then_returns() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySavedSearchQueryRepository::default();
    let query = sample_query(tenant_id);

    let created = repo.create(query.clone()).await.unwrap();
    assert_eq!(created, query);
    assert_eq!(repo.list(tenant_id).await.unwrap(), vec![query]);
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySavedSearchQueryRepository::default();
    repo.create(sample_query(tenant_id)).await.unwrap();
    repo.create(sample_query(Uuid::new_v4())).await.unwrap();

    assert_eq!(repo.list(tenant_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn delete_removes_a_query_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySavedSearchQueryRepository::default();
    let query = sample_query(tenant_id);
    repo.create(query.clone()).await.unwrap();

    repo.delete(tenant_id, query.id).await.unwrap();
    assert!(repo.list(tenant_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_for_a_different_tenant_leaves_it_intact_and_returns_not_found() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySavedSearchQueryRepository::default();
    let query = sample_query(tenant_id);
    repo.create(query.clone()).await.unwrap();

    let err = repo.delete(Uuid::new_v4(), query.id).await;
    assert!(matches!(err, Err(SavedSearchQueryRepositoryError::NotFound(_))));
    assert_eq!(repo.list(tenant_id).await.unwrap(), vec![query]);
}
