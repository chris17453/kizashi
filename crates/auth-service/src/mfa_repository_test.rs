use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryMfaChallengeRepository {
    pub challenges: Mutex<HashMap<String, (Uuid, Uuid)>>,
}

#[async_trait]
impl MfaChallengeRepository for InMemoryMfaChallengeRepository {
    async fn create(&self, user_id: Uuid, tenant_id: Uuid) -> Result<String, MfaRepositoryError> {
        let token = generate_challenge_token();
        self.challenges.lock().unwrap().insert(token.clone(), (user_id, tenant_id));
        Ok(token)
    }

    async fn consume(&self, token: &str) -> Result<Option<(Uuid, Uuid)>, MfaRepositoryError> {
        Ok(self.challenges.lock().unwrap().remove(token))
    }
}

#[tokio::test]
async fn a_challenge_token_can_only_be_consumed_once() {
    let repo = InMemoryMfaChallengeRepository::default();
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let token = repo.create(user_id, tenant_id).await.unwrap();

    let first = repo.consume(&token).await.unwrap();
    let second = repo.consume(&token).await.unwrap();

    assert_eq!(first, Some((user_id, tenant_id)));
    assert_eq!(second, None);
}
