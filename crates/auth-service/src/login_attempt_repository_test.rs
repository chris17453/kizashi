use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryLoginAttemptRepository {
    pub attempts: Mutex<Vec<LoginAttempt>>,
}

#[async_trait]
impl LoginAttemptRepository for InMemoryLoginAttemptRepository {
    async fn record(&self, attempt: &LoginAttempt) -> Result<(), LoginAttemptRepositoryError> {
        self.attempts.lock().unwrap().push(attempt.clone());
        Ok(())
    }

    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptRepositoryError> {
        let mut matching: Vec<LoginAttempt> = self
            .attempts
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.tenant_id == Some(tenant_id))
            .filter(|a| before.is_none_or(|b| a.attempted_at < b))
            .cloned()
            .collect();
        matching.sort_by_key(|a| std::cmp::Reverse(a.attempted_at));
        matching.truncate(limit as usize);
        Ok(matching)
    }
}

pub struct FailingLoginAttemptRepository;

#[async_trait]
impl LoginAttemptRepository for FailingLoginAttemptRepository {
    async fn record(&self, _attempt: &LoginAttempt) -> Result<(), LoginAttemptRepositoryError> {
        Err(LoginAttemptRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_recent(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptRepositoryError> {
        Err(LoginAttemptRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_attempt(tenant_id: Option<Uuid>, username: &str, success: bool) -> LoginAttempt {
    LoginAttempt {
        id: Uuid::new_v4(),
        tenant_id,
        username: username.to_string(),
        success,
        reason: if success { "ok".to_string() } else { "wrong_password".to_string() },
        attempted_at: Utc::now(),
    }
}

#[tokio::test]
async fn list_recent_only_returns_the_given_tenants_attempts() {
    let repo = InMemoryLoginAttemptRepository::default();
    let tenant_a = Uuid::new_v4();
    repo.record(&sample_attempt(Some(tenant_a), "alice", false)).await.unwrap();
    repo.record(&sample_attempt(Some(Uuid::new_v4()), "eve", false)).await.unwrap();

    let found = repo.list_recent(tenant_a, 10, None).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].username, "alice");
}

#[tokio::test]
async fn list_recent_honors_the_limit() {
    let repo = InMemoryLoginAttemptRepository::default();
    let tenant_id = Uuid::new_v4();
    for _ in 0..5 {
        repo.record(&sample_attempt(Some(tenant_id), "alice", false)).await.unwrap();
    }

    let found = repo.list_recent(tenant_id, 2, None).await.unwrap();

    assert_eq!(found.len(), 2);
}
