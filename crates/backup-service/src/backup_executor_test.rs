use super::*;
use crate::backup_run_repository::backup_run_repository_test::{
    FailingBackupRunRepository, InMemoryBackupRunRepository,
};
use crate::backup_store::backup_store_test::{FailingBackupStore, InMemoryBackupStore};
use crate::pg_dump_runner::pg_dump_runner_test::{FailingPgDumpRunner, InMemoryPgDumpRunner};

fn state_with(
    dump_runner: Arc<dyn PgDumpRunner>,
    store: Arc<dyn BackupStore>,
) -> (BackupExecutorState, Arc<InMemoryBackupRunRepository>) {
    let run_repository = Arc::new(InMemoryBackupRunRepository::default());
    (
        BackupExecutorState { run_repository: run_repository.clone(), store, dump_runner },
        run_repository,
    )
}

#[tokio::test]
async fn a_successful_backup_uploads_and_records_success_with_the_dump_size() {
    let (state, repo) = state_with(
        Arc::new(InMemoryPgDumpRunner { bytes: vec![0u8; 128] }),
        Arc::new(InMemoryBackupStore::default()),
    );

    let outcome = run_backup(&state, Utc::now()).await;

    assert_eq!(outcome.status, BackupRunStatus::Success);
    assert_eq!(outcome.size_bytes, Some(128));
    let runs = repo.list_recent(10, None).await.unwrap();
    assert_eq!(runs[0].status, BackupRunStatus::Success);
    assert_eq!(runs[0].size_bytes, Some(128));
}

#[tokio::test]
async fn a_dump_failure_is_recorded_as_a_failed_run() {
    let (state, repo) =
        state_with(Arc::new(FailingPgDumpRunner), Arc::new(InMemoryBackupStore::default()));

    let outcome = run_backup(&state, Utc::now()).await;

    assert_eq!(outcome.status, BackupRunStatus::Failed);
    assert!(outcome.error.is_some());
    let runs = repo.list_recent(10, None).await.unwrap();
    assert_eq!(runs[0].status, BackupRunStatus::Failed);
}

#[tokio::test]
async fn an_upload_failure_is_recorded_as_a_failed_run() {
    let (state, repo) = state_with(
        Arc::new(InMemoryPgDumpRunner { bytes: vec![1, 2, 3] }),
        Arc::new(FailingBackupStore),
    );

    let outcome = run_backup(&state, Utc::now()).await;

    assert_eq!(outcome.status, BackupRunStatus::Failed);
    let runs = repo.list_recent(10, None).await.unwrap();
    assert_eq!(runs[0].status, BackupRunStatus::Failed);
}

#[tokio::test]
async fn a_failure_to_record_the_backup_start_still_returns_a_failed_outcome() {
    let state = BackupExecutorState {
        run_repository: Arc::new(FailingBackupRunRepository),
        store: Arc::new(InMemoryBackupStore::default()),
        dump_runner: Arc::new(InMemoryPgDumpRunner { bytes: vec![1] }),
    };

    let outcome = run_backup(&state, Utc::now()).await;

    assert_eq!(outcome.status, BackupRunStatus::Failed);
    assert!(outcome.error.unwrap().contains("failed to record backup start"));
}

#[tokio::test]
async fn the_uploaded_bytes_are_exactly_what_pg_dump_produced() {
    let store = Arc::new(InMemoryBackupStore::default());
    let (state, _repo) =
        state_with(Arc::new(InMemoryPgDumpRunner { bytes: vec![9, 9, 9] }), store.clone());

    run_backup(&state, Utc::now()).await;

    let uploads = store.uploads.lock().unwrap();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].1, vec![9, 9, 9]);
}
