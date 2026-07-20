#[path = "mfa_repository_test.rs"]
#[cfg(test)]
pub(crate) mod mfa_repository_test;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MfaRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

const CHALLENGE_TTL_MINUTES: i64 = 5;

/// Bridges `local_login`'s password check and `mfa/challenge`'s code check across two separate
/// HTTP round trips (ADR-0051) -- a random opaque token mapped server-side to which user is
/// mid-login, expiring after `CHALLENGE_TTL_MINUTES` so an abandoned login attempt can't be
/// resumed indefinitely.
#[async_trait]
pub trait MfaChallengeRepository: Send + Sync {
    async fn create(&self, user_id: Uuid, tenant_id: Uuid) -> Result<String, MfaRepositoryError>;

    /// Consumes (deletes) the challenge on read, whether or not it was found/valid -- a
    /// challenge token is single-use by construction, preventing replay of a stolen token even
    /// if the attacker also somehow obtained a valid code.
    async fn consume(&self, token: &str) -> Result<Option<(Uuid, Uuid)>, MfaRepositoryError>;
}

pub struct PostgresMfaChallengeRepository {
    pool: sqlx::PgPool,
}

impl PostgresMfaChallengeRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

fn generate_challenge_token() -> String {
    Uuid::new_v4().to_string()
}

#[async_trait]
impl MfaChallengeRepository for PostgresMfaChallengeRepository {
    async fn create(&self, user_id: Uuid, tenant_id: Uuid) -> Result<String, MfaRepositoryError> {
        let token = generate_challenge_token();
        let now: DateTime<Utc> = Utc::now();
        sqlx::query(
            "INSERT INTO mfa_challenges (id, local_user_id, tenant_id, challenge_token, created_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(tenant_id)
        .bind(&token)
        .bind(now)
        .bind(now + Duration::minutes(CHALLENGE_TTL_MINUTES))
        .execute(&self.pool)
        .await
        .map_err(|e| MfaRepositoryError::Backend(e.to_string()))?;
        Ok(token)
    }

    async fn consume(&self, token: &str) -> Result<Option<(Uuid, Uuid)>, MfaRepositoryError> {
        let row: Option<(Uuid, Uuid, DateTime<Utc>)> = sqlx::query_as(
            "DELETE FROM mfa_challenges WHERE challenge_token = $1 RETURNING local_user_id, tenant_id, expires_at",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| MfaRepositoryError::Backend(e.to_string()))?;

        Ok(row.and_then(
            |(user_id, tenant_id, expires_at)| {
                if expires_at >= Utc::now() {
                    Some((user_id, tenant_id))
                } else {
                    None
                }
            },
        ))
    }
}
