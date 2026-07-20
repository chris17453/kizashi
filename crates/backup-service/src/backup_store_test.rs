use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryBackupStore {
    pub uploads: Mutex<Vec<(String, Vec<u8>)>>,
}

#[async_trait]
impl BackupStore for InMemoryBackupStore {
    async fn upload(&self, key: &str, bytes: Vec<u8>) -> Result<(), BackupStoreError> {
        self.uploads.lock().unwrap().push((key.to_string(), bytes));
        Ok(())
    }
}

pub struct FailingBackupStore;

#[async_trait]
impl BackupStore for FailingBackupStore {
    async fn upload(&self, _key: &str, _bytes: Vec<u8>) -> Result<(), BackupStoreError> {
        Err(BackupStoreError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn upload_records_the_key_and_bytes() {
    let store = InMemoryBackupStore::default();

    store.upload("backups/2026-07-20.sql.gz", vec![1, 2, 3]).await.unwrap();

    let uploads = store.uploads.lock().unwrap();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].0, "backups/2026-07-20.sql.gz");
    assert_eq!(uploads[0].1, vec![1, 2, 3]);
}
