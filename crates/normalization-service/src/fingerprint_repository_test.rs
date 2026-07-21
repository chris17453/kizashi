use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryFingerprintRepository {
    seen: Mutex<HashMap<(Uuid, String), chrono::DateTime<chrono::Utc>>>,
}

#[async_trait]
impl FingerprintRepository for InMemoryFingerprintRepository {
    async fn check_and_record(
        &self,
        tenant_id: Uuid,
        fingerprint: &str,
        _record_id: Uuid,
        window_seconds: Option<i64>,
    ) -> Result<DedupOutcome, FingerprintRepositoryError> {
        let now = chrono::Utc::now();
        let mut seen = self.seen.lock().unwrap();
        let key = (tenant_id, fingerprint.to_string());
        let outcome = match seen.get(&key) {
            None => DedupOutcome::New,
            Some(last_seen_at) => {
                let within_window = match window_seconds {
                    None => true,
                    Some(window) => (now - *last_seen_at).num_seconds() < window,
                };
                if within_window {
                    DedupOutcome::Duplicate
                } else {
                    DedupOutcome::New
                }
            }
        };
        seen.insert(key, now);
        Ok(outcome)
    }
}

pub struct FailingFingerprintRepository;

#[async_trait]
impl FingerprintRepository for FailingFingerprintRepository {
    async fn check_and_record(
        &self,
        _tenant_id: Uuid,
        _fingerprint: &str,
        _record_id: Uuid,
        _window_seconds: Option<i64>,
    ) -> Result<DedupOutcome, FingerprintRepositoryError> {
        Err(FingerprintRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn first_sighting_of_a_fingerprint_is_new() {
    let repo = InMemoryFingerprintRepository::default();
    let outcome = repo.check_and_record(Uuid::new_v4(), "abc", Uuid::new_v4(), None).await.unwrap();
    assert_eq!(outcome, DedupOutcome::New);
}

#[tokio::test]
async fn a_second_sighting_within_the_window_is_a_duplicate() {
    let repo = InMemoryFingerprintRepository::default();
    let tenant_id = Uuid::new_v4();
    repo.check_and_record(tenant_id, "abc", Uuid::new_v4(), Some(3600)).await.unwrap();

    let outcome =
        repo.check_and_record(tenant_id, "abc", Uuid::new_v4(), Some(3600)).await.unwrap();
    assert_eq!(outcome, DedupOutcome::Duplicate);
}

#[tokio::test]
async fn a_second_sighting_with_no_window_expiry_is_always_a_duplicate() {
    let repo = InMemoryFingerprintRepository::default();
    let tenant_id = Uuid::new_v4();
    repo.check_and_record(tenant_id, "abc", Uuid::new_v4(), None).await.unwrap();

    let outcome = repo.check_and_record(tenant_id, "abc", Uuid::new_v4(), None).await.unwrap();
    assert_eq!(outcome, DedupOutcome::Duplicate);
}

#[tokio::test]
async fn different_tenants_do_not_share_fingerprint_state() {
    let repo = InMemoryFingerprintRepository::default();
    repo.check_and_record(Uuid::new_v4(), "abc", Uuid::new_v4(), Some(3600)).await.unwrap();

    let outcome =
        repo.check_and_record(Uuid::new_v4(), "abc", Uuid::new_v4(), Some(3600)).await.unwrap();
    assert_eq!(outcome, DedupOutcome::New);
}

#[tokio::test]
async fn different_fingerprints_do_not_collide() {
    let repo = InMemoryFingerprintRepository::default();
    let tenant_id = Uuid::new_v4();
    repo.check_and_record(tenant_id, "abc", Uuid::new_v4(), Some(3600)).await.unwrap();

    let outcome =
        repo.check_and_record(tenant_id, "xyz", Uuid::new_v4(), Some(3600)).await.unwrap();
    assert_eq!(outcome, DedupOutcome::New);
}

#[tokio::test]
async fn failing_repository_returns_a_backend_error() {
    let repo = FailingFingerprintRepository;
    let err = repo.check_and_record(Uuid::new_v4(), "abc", Uuid::new_v4(), None).await.unwrap_err();
    assert!(matches!(err, FingerprintRepositoryError::Backend(_)));
}
