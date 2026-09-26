//! Postgres [`SubjectKeyStore`]. Integration-tested against real Postgres in
//! `tests/pg_key_store_integration_test.rs`. Every mutation and its audit row share one
//! transaction, so there is never a key change without an audit record.

use crate::{
    CryptoError, KekId, KeyAuditAction, KeyAuditEntry, ShredResult, StoredSubjectKey,
    SubjectKeyStore, SubjectRef,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Embedded migrations for `yocho_subject_keys` and `yocho_key_audit`. Hosts run this against
/// their own schema-scoped pool at startup.
pub fn migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate!("./migrations")
}

pub struct PgSubjectKeyStore {
    pool: PgPool,
}

impl PgSubjectKeyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type KeyRow =
    (Uuid, String, String, Option<Vec<u8>>, Option<String>, DateTime<Utc>, Option<DateTime<Utc>>);

fn from_row(row: KeyRow) -> StoredSubjectKey {
    let (tenant_id, subject_type, subject_id, wrapped_dek, kek_id, created_at, shredded_at) = row;
    StoredSubjectKey {
        subject: SubjectRef::new(tenant_id, subject_type, subject_id),
        wrapped_dek,
        kek_id: kek_id.map(KekId::new),
        created_at,
        shredded_at,
    }
}

async fn write_audit(
    tx: &mut Transaction<'_, Postgres>,
    subject: &SubjectRef,
    action: KeyAuditAction,
    kek_id: Option<&str>,
    actor: &str,
) -> Result<(), CryptoError> {
    sqlx::query(
        "INSERT INTO yocho_key_audit (id, tenant_id, subject_type, subject_id, action, kek_id, actor)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(subject.tenant_id)
    .bind(&subject.subject_type)
    .bind(&subject.subject_id)
    .bind(action.as_str())
    .bind(kek_id)
    .bind(actor)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

const SELECT_KEY: &str =
    "SELECT tenant_id, subject_type, subject_id, wrapped_dek, kek_id, created_at, shredded_at
     FROM yocho_subject_keys WHERE tenant_id = $1 AND subject_type = $2 AND subject_id = $3";

#[async_trait]
impl SubjectKeyStore for PgSubjectKeyStore {
    async fn get(&self, subject: &SubjectRef) -> Result<Option<StoredSubjectKey>, CryptoError> {
        let row: Option<KeyRow> = sqlx::query_as(SELECT_KEY)
            .bind(subject.tenant_id)
            .bind(&subject.subject_type)
            .bind(&subject.subject_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(from_row))
    }

    async fn insert_if_absent(
        &self,
        subject: &SubjectRef,
        wrapped_dek: &[u8],
        kek_id: &KekId,
        actor: &str,
    ) -> Result<StoredSubjectKey, CryptoError> {
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO yocho_subject_keys (tenant_id, subject_type, subject_id, wrapped_dek, kek_id)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (tenant_id, subject_type, subject_id) DO NOTHING",
        )
        .bind(subject.tenant_id)
        .bind(&subject.subject_type)
        .bind(&subject.subject_id)
        .bind(wrapped_dek)
        .bind(kek_id.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if inserted {
            write_audit(&mut tx, subject, KeyAuditAction::Created, Some(kek_id.as_str()), actor)
                .await?;
        }
        let row: KeyRow = sqlx::query_as(SELECT_KEY)
            .bind(subject.tenant_id)
            .bind(&subject.subject_type)
            .bind(&subject.subject_id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(from_row(row))
    }

    async fn shred(&self, subject: &SubjectRef, actor: &str) -> Result<ShredResult, CryptoError> {
        let mut tx = self.pool.begin().await?;
        let newly: Option<(DateTime<Utc>, Option<String>)> = sqlx::query_as(
            "INSERT INTO yocho_subject_keys (tenant_id, subject_type, subject_id, wrapped_dek, kek_id, shredded_at)
             VALUES ($1, $2, $3, NULL, NULL, now())
             ON CONFLICT (tenant_id, subject_type, subject_id)
             DO UPDATE SET wrapped_dek = NULL, shredded_at = now()
             WHERE yocho_subject_keys.shredded_at IS NULL
             RETURNING shredded_at, kek_id",
        )
        .bind(subject.tenant_id)
        .bind(&subject.subject_type)
        .bind(&subject.subject_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (result, kek_id) = match newly {
            Some((at, kek)) => (ShredResult { shredded_at: at, newly_shredded: true }, kek),
            None => {
                let row: KeyRow = sqlx::query_as(SELECT_KEY)
                    .bind(subject.tenant_id)
                    .bind(&subject.subject_type)
                    .bind(&subject.subject_id)
                    .fetch_one(&mut *tx)
                    .await?;
                let row = from_row(row);
                let at = row.shredded_at.ok_or_else(|| {
                    CryptoError::Store("shred conflict on a row that is not shredded".into())
                })?;
                (
                    ShredResult { shredded_at: at, newly_shredded: false },
                    row.kek_id.map(|k| k.as_str().to_string()),
                )
            }
        };
        let action = if result.newly_shredded {
            KeyAuditAction::Shredded
        } else {
            KeyAuditAction::ShredRepeated
        };
        write_audit(&mut tx, subject, action, kek_id.as_deref(), actor).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn audit_log(&self, subject: &SubjectRef) -> Result<Vec<KeyAuditEntry>, CryptoError> {
        let rows: Vec<(Uuid, String, Option<String>, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, action, kek_id, actor, occurred_at FROM yocho_key_audit
             WHERE tenant_id = $1 AND subject_type = $2 AND subject_id = $3
             ORDER BY occurred_at, id",
        )
        .bind(subject.tenant_id)
        .bind(&subject.subject_type)
        .bind(&subject.subject_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, action, kek_id, actor, occurred_at)| {
                let action = KeyAuditAction::parse(&action)
                    .ok_or_else(|| CryptoError::Store(format!("unknown audit action {action}")))?;
                Ok(KeyAuditEntry {
                    id,
                    subject: subject.clone(),
                    action,
                    kek_id: kek_id.map(KekId::new),
                    actor,
                    occurred_at,
                })
            })
            .collect()
    }
}
