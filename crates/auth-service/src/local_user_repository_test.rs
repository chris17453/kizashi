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

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<LocalUser>, LocalUserRepositoryError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn create(&self, user: LocalUser) -> Result<LocalUser, LocalUserRepositoryError> {
        self.users.lock().unwrap().push(user.clone());
        Ok(user)
    }

    async fn update_role(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        role: Role,
        _actor: &str,
    ) -> Result<LocalUser, LocalUserRepositoryError> {
        let mut users = self.users.lock().unwrap();
        let user = users
            .iter_mut()
            .find(|u| u.id == id && u.tenant_id == tenant_id)
            .ok_or(LocalUserRepositoryError::NotFound(id))?;
        user.role = role;
        Ok(user.clone())
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        _actor: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        let mut users = self.users.lock().unwrap();
        let before_len = users.len();
        users.retain(|u| !(u.id == id && u.tenant_id == tenant_id));
        if users.len() == before_len {
            return Err(LocalUserRepositoryError::NotFound(id));
        }
        Ok(())
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

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<LocalUser>, LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn create(&self, _user: LocalUser) -> Result<LocalUser, LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update_role(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _role: Role,
        _actor: &str,
    ) -> Result<LocalUser, LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn delete(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _actor: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_user(tenant_id: Uuid) -> LocalUser {
    LocalUser {
        id: Uuid::new_v4(),
        tenant_id,
        username: "alice".to_string(),
        password_hash: "hash".to_string(),
        role: common::Role::Operator,
    }
}

#[tokio::test]
async fn finds_a_user_scoped_to_tenant_and_username() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
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
        role: common::Role::Viewer,
    };
    let repo = InMemoryLocalUserRepository::with_user(user);

    let found = repo.find_by_username(Uuid::new_v4(), "alice").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn list_returns_only_users_for_the_given_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryLocalUserRepository::default();
    repo.create(sample_user(tenant_id)).await.unwrap();
    repo.create(sample_user(Uuid::new_v4())).await.unwrap();

    let found = repo.list(tenant_id).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn create_adds_a_user_that_can_then_be_found() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryLocalUserRepository::default();
    let user = sample_user(tenant_id);

    let created = repo.create(user.clone()).await.unwrap();
    assert_eq!(created, user);
    assert_eq!(repo.list(tenant_id).await.unwrap(), vec![user]);
}

#[tokio::test]
async fn update_role_changes_the_role_and_leaves_other_fields_intact() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
    let repo = InMemoryLocalUserRepository::with_user(user.clone());

    let updated = repo.update_role(tenant_id, user.id, Role::Admin, "actor").await.unwrap();
    assert_eq!(updated.role, Role::Admin);
    assert_eq!(updated.username, user.username);
}

#[tokio::test]
async fn update_role_for_an_unknown_id_returns_not_found() {
    let repo = InMemoryLocalUserRepository::default();
    let err = repo.update_role(Uuid::new_v4(), Uuid::new_v4(), Role::Admin, "actor").await;
    assert!(matches!(err, Err(LocalUserRepositoryError::NotFound(_))));
}

#[tokio::test]
async fn delete_removes_a_user_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
    let repo = InMemoryLocalUserRepository::with_user(user.clone());

    repo.delete(tenant_id, user.id, "actor").await.unwrap();
    assert_eq!(repo.list(tenant_id).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn delete_for_a_different_tenant_leaves_the_user_intact_and_returns_not_found() {
    let tenant_id = Uuid::new_v4();
    let user = sample_user(tenant_id);
    let repo = InMemoryLocalUserRepository::with_user(user.clone());

    let err = repo.delete(Uuid::new_v4(), user.id, "actor").await;
    assert!(matches!(err, Err(LocalUserRepositoryError::NotFound(_))));
    assert_eq!(repo.list(tenant_id).await.unwrap(), vec![user]);
}
