#[path = "archive_store_test.rs"]
#[cfg(test)]
pub(crate) mod archive_store_test;

use crate::manifest::{archive_key, ArchiveManifest};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::RawRecord;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ArchiveStoreError {
    #[error("archive backend error: {0}")]
    Backend(String),
    #[error("no archive batch at key {0}")]
    NotFound(String),
    #[error("archive batch at key {0} is corrupt: {1}")]
    Corrupt(String, String),
}

/// Writes/reads NDJSON+gzip archive batches per ADR-0005. `write_batch` returns the object key
/// so callers (sweep.rs) can record where a batch landed; `read_batch` is reimport's read path.
#[async_trait]
pub trait ArchiveStore: Send + Sync {
    async fn write_batch(
        &self,
        tenant_id: Uuid,
        data_class: &str,
        records: &[RawRecord],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<String, ArchiveStoreError>;

    async fn read_batch(
        &self,
        key: &str,
    ) -> Result<(ArchiveManifest, Vec<RawRecord>), ArchiveStoreError>;
}

/// Encodes a manifest line followed by one `RawRecord` JSON line per record, gzip-compressed —
/// shared by every `ArchiveStore` backend so the wire format can't drift between them.
fn encode_batch(
    manifest: &ArchiveManifest,
    records: &[RawRecord],
) -> Result<Vec<u8>, ArchiveStoreError> {
    let mut ndjson =
        serde_json::to_string(manifest).map_err(|e| ArchiveStoreError::Backend(e.to_string()))?;
    ndjson.push('\n');
    for record in records {
        ndjson.push_str(
            &serde_json::to_string(record)
                .map_err(|e| ArchiveStoreError::Backend(e.to_string()))?,
        );
        ndjson.push('\n');
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(ndjson.as_bytes()).map_err(|e| ArchiveStoreError::Backend(e.to_string()))?;
    encoder.finish().map_err(|e| ArchiveStoreError::Backend(e.to_string()))
}

/// Decodes a gzip archive batch back into its manifest and records — the inverse of
/// `encode_batch`, shared by every backend.
fn decode_batch(
    key: &str,
    gzipped: &[u8],
) -> Result<(ArchiveManifest, Vec<RawRecord>), ArchiveStoreError> {
    let mut decoder = GzDecoder::new(gzipped);
    let mut ndjson = String::new();
    decoder
        .read_to_string(&mut ndjson)
        .map_err(|e| ArchiveStoreError::Corrupt(key.to_string(), e.to_string()))?;

    let mut lines = ndjson.lines();
    let manifest_line = lines
        .next()
        .ok_or_else(|| ArchiveStoreError::Corrupt(key.to_string(), "empty batch".to_string()))?;
    let manifest: ArchiveManifest = serde_json::from_str(manifest_line)
        .map_err(|e| ArchiveStoreError::Corrupt(key.to_string(), e.to_string()))?;

    let records = lines
        .map(|line| {
            serde_json::from_str::<RawRecord>(line)
                .map_err(|e| ArchiveStoreError::Corrupt(key.to_string(), e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok((manifest, records))
}

/// S3-compatible archival backend (ADR-0011) — talks to real AWS S3 or a self-hosted
/// S3-compatible target (MinIO in docker-compose) via the AWS SDK's standard endpoint
/// override, so the same code path serves both without a feature flag.
pub struct S3ArchiveStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3ArchiveStore {
    pub fn new(client: aws_sdk_s3::Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Idempotently ensures the archive bucket exists — safe to call on every startup.
    pub async fn ensure_bucket(&self) -> Result<(), ArchiveStoreError> {
        match self.client.create_bucket().bucket(&self.bucket).send().await {
            Ok(_) => Ok(()),
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_bucket_already_owned_by_you()
                    || service_err.is_bucket_already_exists()
                {
                    Ok(())
                } else {
                    Err(ArchiveStoreError::Backend(service_err.to_string()))
                }
            }
        }
    }
}

#[async_trait]
impl ArchiveStore for S3ArchiveStore {
    async fn write_batch(
        &self,
        tenant_id: Uuid,
        data_class: &str,
        records: &[RawRecord],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<String, ArchiveStoreError> {
        let manifest =
            ArchiveManifest::new(tenant_id, data_class, records.len(), window_start, window_end);
        let gzipped = encode_batch(&manifest, records)?;
        let batch_id = Uuid::new_v4();
        let key = archive_key(tenant_id, data_class, window_end, batch_id);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(gzipped.into())
            .send()
            .await
            .map_err(|e| ArchiveStoreError::Backend(e.to_string()))?;

        Ok(key)
    }

    async fn read_batch(
        &self,
        key: &str,
    ) -> Result<(ArchiveManifest, Vec<RawRecord>), ArchiveStoreError> {
        let response =
            self.client.get_object().bucket(&self.bucket).key(key).send().await.map_err(|e| {
                let service_err = e.into_service_error();
                if service_err.is_no_such_key() {
                    ArchiveStoreError::NotFound(key.to_string())
                } else {
                    ArchiveStoreError::Backend(service_err.to_string())
                }
            })?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| ArchiveStoreError::Backend(e.to_string()))?
            .into_bytes();

        decode_batch(key, &bytes)
    }
}
