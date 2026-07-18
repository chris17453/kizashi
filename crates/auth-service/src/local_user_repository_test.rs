use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryLocalUserRepository {
    pub users: Mutex<Vec<LocalUser>>,
}

impl InMemoryLocalUserRepository {
    pub fn with_user(user: LocalUser) -> Self {
        Self { users: Mutex::new(vec![user]) }
    }
}

#[async_trait]
impl LocalUserRepository for InMemoryLocalUserRepository {
    async fn find_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<LocalUser>, LocalUserRepositoryError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.tenant_id == tenant_id && u.username == username)
            .cloned())
    }
}

pub struct FailingLocalUserRepository;

#[async_trait]
impl LocalUserRepository for FailingLocalUserRepository {
    async fn find_by_username(
        &self,
        _tenant_id: Uuid,
        _username: &str,
    ) -> Result<Option<LocalUser>, LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn finds_a_user_scoped_to_tenant_and_username() {
    let tenant_id = Uuid::new_v4();
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "alice".to_string(),
        password_hash: "hash".to_string(),
    };
    let repo = InMemoryLocalUserRepository::with_user(user.clone());

    let found = repo.find_by_username(tenant_id, "alice").await.unwrap();
    assert_eq!(found, Some(user));
}

#[tokio::test]
async fn does_not_find_the_same_username_in_a_different_tenant() {
    let user = LocalUser {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        username: "alice".to_string(),
        password_hash: "hash".to_string(),
    };
    let repo = InMemoryLocalUserRepository::with_user(user);

    let found = repo.find_by_username(Uuid::new_v4(), "alice").await.unwrap();
    assert!(found.is_none());
}
