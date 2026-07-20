#[path = "backup_store_test.rs"]
#[cfg(test)]
pub(crate) mod backup_store_test;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupStoreError {
    #[error("backup storage backend error: {0}")]
    Backend(String),
}

/// Where a completed `pg_dump` blob lands (ADR-0055) — an S3-compatible bucket (MinIO in
/// docker-compose, real S3 in production), same storage class as `retention-service`'s
/// `ArchiveStore` but a separate bucket: retention's archive is tenant application data under a
/// retention policy, this is a whole-database ops backup, different lifecycle and owner.
#[async_trait]
pub trait BackupStore: Send + Sync {
    async fn upload(&self, key: &str, bytes: Vec<u8>) -> Result<(), BackupStoreError>;
}

pub struct S3BackupStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3BackupStore {
    pub fn new(client: aws_sdk_s3::Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Idempotently ensures the backup bucket exists — safe to call on every startup, mirrors
    /// `S3ArchiveStore::ensure_bucket`.
    pub async fn ensure_bucket(&self) -> Result<(), BackupStoreError> {
        match self.client.create_bucket().bucket(&self.bucket).send().await {
            Ok(_) => Ok(()),
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_bucket_already_owned_by_you()
                    || service_err.is_bucket_already_exists()
                {
                    Ok(())
                } else {
                    Err(BackupStoreError::Backend(service_err.to_string()))
                }
            }
        }
    }
}

#[async_trait]
impl BackupStore for S3BackupStore {
    async fn upload(&self, key: &str, bytes: Vec<u8>) -> Result<(), BackupStoreError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(bytes.into())
            .send()
            .await
            .map_err(|e| BackupStoreError::Backend(e.to_string()))?;
        Ok(())
    }
}
