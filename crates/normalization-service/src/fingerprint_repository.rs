#[path = "fingerprint_repository_test.rs"]
#[cfg(test)]
pub(crate) mod fingerprint_repository_test;

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum FingerprintRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupOutcome {
    /// First time this fingerprint has been seen (or the previous sighting fell outside
    /// `window_seconds`) — proceed as a normal, fresh occurrence.
    New,
    /// An exact duplicate within the dedup window — the caller should suppress republishing
    /// `record.normalized` for this record (ADR-0112).
    Duplicate,
}

/// Tracks exact-duplicate fingerprints per tenant (ADR-0112) — not audit-logged like operator
/// config entities, since this is high-churn pipeline state, not something an operator
/// authored.
#[async_trait]
pub trait FingerprintRepository: Send + Sync {
    /// Atomically checks whether `fingerprint` was already seen for `tenant_id` within
    /// `window_seconds` (`None` = no expiry, a fingerprint is remembered forever) and records
    /// this sighting either way. Returns `Duplicate` when the caller should suppress this
    /// record's `record.normalized` publish.
    async fn check_and_record(
        &self,
        tenant_id: Uuid,
        fingerprint: &str,
        record_id: Uuid,
        window_seconds: Option<i64>,
    ) -> Result<DedupOutcome, FingerprintRepositoryError>;
}

pub struct PostgresFingerprintRepository {
    pool: sqlx::PgPool,
}

impl PostgresFingerprintRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FingerprintRepository for PostgresFingerprintRepository {
    async fn check_and_record(
        &self,
        tenant_id: Uuid,
        fingerprint: &str,
        record_id: Uuid,
        window_seconds: Option<i64>,
    ) -> Result<DedupOutcome, FingerprintRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| FingerprintRepositoryError::Backend(e.to_string()))?;

        // FOR UPDATE: two concurrent messages with the same fingerprint must not both observe
        // "no existing row" and both insert — the row lock serializes them so the second
        // consistently sees the first's insert as an existing row to update instead.
        let existing: Option<(chrono::DateTime<chrono::Utc>,)> = sqlx::query_as(
            "SELECT last_seen_at FROM record_fingerprints WHERE tenant_id = $1 AND fingerprint = $2 FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(fingerprint)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| FingerprintRepositoryError::Backend(e.to_string()))?;

        let now = chrono::Utc::now();

        let outcome = match existing {
            None => {
                sqlx::query(
                    r#"
                    INSERT INTO record_fingerprints
                        (tenant_id, fingerprint, first_seen_record_id, last_seen_record_id,
                         occurrence_count, first_seen_at, last_seen_at)
                    VALUES ($1, $2, $3, $3, 1, $4, $4)
                    "#,
                )
                .bind(tenant_id)
                .bind(fingerprint)
                .bind(record_id)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|e| FingerprintRepositoryError::Backend(e.to_string()))?;
                DedupOutcome::New
            }
            Some((last_seen_at,)) => {
                let within_window = match window_seconds {
                    None => true,
                    Some(window) => (now - last_seen_at).num_seconds() < window,
                };
                if within_window {
                    sqlx::query(
                        r#"
                        UPDATE record_fingerprints
                        SET last_seen_record_id = $3, occurrence_count = occurrence_count + 1, last_seen_at = $4
                        WHERE tenant_id = $1 AND fingerprint = $2
                        "#,
                    )
                    .bind(tenant_id)
                    .bind(fingerprint)
                    .bind(record_id)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| FingerprintRepositoryError::Backend(e.to_string()))?;
                    DedupOutcome::Duplicate
                } else {
                    sqlx::query(
                        r#"
                        UPDATE record_fingerprints
                        SET first_seen_record_id = $3, last_seen_record_id = $3,
                            occurrence_count = 1, first_seen_at = $4, last_seen_at = $4
                        WHERE tenant_id = $1 AND fingerprint = $2
                        "#,
                    )
                    .bind(tenant_id)
                    .bind(fingerprint)
                    .bind(record_id)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| FingerprintRepositoryError::Backend(e.to_string()))?;
                    DedupOutcome::New
                }
            }
        };

        tx.commit().await.map_err(|e| FingerprintRepositoryError::Backend(e.to_string()))?;
        Ok(outcome)
    }
}
