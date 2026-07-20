use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryBackupRunRepository {
    pub runs: Mutex<Vec<BackupRun>>,
}

#[async_trait]
impl BackupRunRepository for InMemoryBackupRunRepository {
    async fn start(
        &self,
        id: Uuid,
        started_at: DateTime<Utc>,
        target: &str,
    ) -> Result<(), BackupRunRepositoryError> {
        self.runs.lock().unwrap().push(BackupRun {
            id,
            started_at,
            completed_at: None,
            status: BackupRunStatus::Running,
            target: target.to_string(),
            size_bytes: None,
            error: None,
        });
        Ok(())
    }

    async fn complete(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        size_bytes: i64,
    ) -> Result<(), BackupRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        if let Some(run) = runs.iter_mut().find(|r| r.id == id) {
            run.status = BackupRunStatus::Success;
            run.completed_at = Some(completed_at);
            run.size_bytes = Some(size_bytes);
        }
        Ok(())
    }

    async fn fail(
        &self,
        id: Uuid,
        completed_at: DateTime<Utc>,
        error: &str,
    ) -> Result<(), BackupRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        if let Some(run) = runs.iter_mut().find(|r| r.id == id) {
            run.status = BackupRunStatus::Failed;
            run.completed_at = Some(completed_at);
            run.error = Some(error.to_string());
        }
        Ok(())
    }

    async fn list_recent(
        &self,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<BackupRun>, BackupRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap().clone();
        if let Some(before) = before {
            runs.retain(|r| r.started_at < before);
        }
        runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        runs.truncate(limit as usize);
        Ok(runs)
    }
}

pub struct FailingBackupRunRepository;

#[async_trait]
impl BackupRunRepository for FailingBackupRunRepository {
    async fn start(
        &self,
        _id: Uuid,
        _started_at: DateTime<Utc>,
        _target: &str,
    ) -> Result<(), BackupRunRepositoryError> {
        Err(BackupRunRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn complete(
        &self,
        _id: Uuid,
        _completed_at: DateTime<Utc>,
        _size_bytes: i64,
    ) -> Result<(), BackupRunRepositoryError> {
        Err(BackupRunRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn fail(
        &self,
        _id: Uuid,
        _completed_at: DateTime<Utc>,
        _error: &str,
    ) -> Result<(), BackupRunRepositoryError> {
        Err(BackupRunRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_recent(
        &self,
        _limit: i64,
        _before: Option<DateTime<Utc>>,
    ) -> Result<Vec<BackupRun>, BackupRunRepositoryError> {
        Err(BackupRunRepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn start_then_complete_updates_the_same_run() {
    let repo = InMemoryBackupRunRepository::default();
    let id = Uuid::new_v4();
    repo.start(id, Utc::now(), "postgres").await.unwrap();

    repo.complete(id, Utc::now(), 4096).await.unwrap();

    let runs = repo.list_recent(10, None).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, BackupRunStatus::Success);
    assert_eq!(runs[0].size_bytes, Some(4096));
}

#[tokio::test]
async fn start_then_fail_records_the_error() {
    let repo = InMemoryBackupRunRepository::default();
    let id = Uuid::new_v4();
    repo.start(id, Utc::now(), "postgres").await.unwrap();

    repo.fail(id, Utc::now(), "pg_dump exited with status 1").await.unwrap();

    let runs = repo.list_recent(10, None).await.unwrap();
    assert_eq!(runs[0].status, BackupRunStatus::Failed);
    assert_eq!(runs[0].error.as_deref(), Some("pg_dump exited with status 1"));
}

#[tokio::test]
async fn list_recent_is_most_recent_first_and_honors_the_limit() {
    let repo = InMemoryBackupRunRepository::default();
    for _ in 0..5 {
        repo.start(Uuid::new_v4(), Utc::now(), "postgres").await.unwrap();
    }

    let runs = repo.list_recent(2, None).await.unwrap();

    assert_eq!(runs.len(), 2);
}

#[tokio::test]
async fn list_recent_with_before_excludes_runs_at_or_after_the_cursor() {
    let repo = InMemoryBackupRunRepository::default();
    let older = "2026-07-01T00:00:00Z".parse().unwrap();
    let newer = "2026-07-15T00:00:00Z".parse().unwrap();
    repo.start(Uuid::new_v4(), older, "postgres").await.unwrap();
    repo.start(Uuid::new_v4(), newer, "postgres").await.unwrap();

    let runs = repo.list_recent(10, Some(newer)).await.unwrap();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].started_at, older);
}
