#[path = "login_attempt_repository_test.rs"]
#[cfg(test)]
pub(crate) mod login_attempt_repository_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum LoginAttemptRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LoginAttempt {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub username: String,
    pub success: bool,
    pub reason: String,
    pub attempted_at: DateTime<Utc>,
}

/// Records and reads `login_attempts` (ADR-0053) -- every local-login and MFA-challenge attempt,
/// so an admin can see a brute-force pattern or a specific account under attack. A record is
/// never mutated after being written; the DB-level trigger in the migration is the actual
/// enforcement (CLAUDE.md §5), this trait just never exposes an update/delete method to begin
/// with.
#[async_trait]
pub trait LoginAttemptRepository: Send + Sync {
    async fn record(&self, attempt: &LoginAttempt) -> Result<(), LoginAttemptRepositoryError>;

    /// `tenant_id: None` lists attempts against unknown workspace names platform-wide (no real
    /// tenant to scope them to); `Some(id)` scopes to one tenant, the shape the Security page
    /// actually uses.
    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptRepositoryError>;

    /// Every attempt recorded against this exact username, across all time -- the data-subject
    /// export path (ADR-0054) needs "everything about this account," not a recency-bounded page.
    /// `username` isn't a foreign key to `local_users.id` (a deleted account's username can still
    /// have historical rows), so this is a plain string match, not a tenant-scoped join.
    async fn list_by_username(
        &self,
        username: &str,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptRepositoryError>;
}

pub struct PostgresLoginAttemptRepository {
    pool: sqlx::PgPool,
}

impl PostgresLoginAttemptRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type AttemptRow = (Uuid, Option<Uuid>, String, bool, String, DateTime<Utc>);

fn row_to_attempt(row: AttemptRow) -> LoginAttempt {
    let (id, tenant_id, username, success, reason, attempted_at) = row;
    LoginAttempt { id, tenant_id, username, success, reason, attempted_at }
}

#[async_trait]
impl LoginAttemptRepository for PostgresLoginAttemptRepository {
    async fn record(&self, attempt: &LoginAttempt) -> Result<(), LoginAttemptRepositoryError> {
        sqlx::query(
            "INSERT INTO login_attempts (id, tenant_id, username, success, reason, attempted_at) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(attempt.id)
        .bind(attempt.tenant_id)
        .bind(&attempt.username)
        .bind(attempt.success)
        .bind(&attempt.reason)
        .bind(attempt.attempted_at)
        .execute(&self.pool)
        .await
        .map_err(|e| LoginAttemptRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_recent(
        &self,
        tenant_id: Uuid,
        limit: i64,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptRepositoryError> {
        let rows: Vec<AttemptRow> = if let Some(before) = before {
            sqlx::query_as(
                "SELECT id, tenant_id, username, success, reason, attempted_at FROM login_attempts WHERE tenant_id = $1 AND attempted_at < $2 ORDER BY attempted_at DESC LIMIT $3",
            )
            .bind(tenant_id)
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT id, tenant_id, username, success, reason, attempted_at FROM login_attempts WHERE tenant_id = $1 ORDER BY attempted_at DESC LIMIT $2",
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| LoginAttemptRepositoryError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_attempt).collect())
    }

    async fn list_by_username(
        &self,
        username: &str,
    ) -> Result<Vec<LoginAttempt>, LoginAttemptRepositoryError> {
        let rows: Vec<AttemptRow> = sqlx::query_as(
            "SELECT id, tenant_id, username, success, reason, attempted_at FROM login_attempts WHERE username = $1 ORDER BY attempted_at DESC",
        )
        .bind(username)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LoginAttemptRepositoryError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_attempt).collect())
    }
}
