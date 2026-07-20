#[path = "local_user_repository_test.rs"]
#[cfg(test)]
pub(crate) mod local_user_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum LocalUserRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no local user with id {0}")]
    NotFound(Uuid),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocalUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    #[serde(skip)]
    pub password_hash: String,
    pub role: Role,
    /// Never serialized (same reasoning as `password_hash`) -- only `mfa.rs`/`mfa_handler.rs`
    /// ever read this to verify a code. `Some` while enrollment is pending confirmation, same
    /// as once confirmed; `mfa_enabled` (not the presence of a secret) is what `local_login`
    /// actually gates on.
    #[serde(skip)]
    pub mfa_secret: Option<String>,
    pub mfa_enabled: bool,
}

/// Local login credential store (spec §8: "local login... hashed credentials"), and (ADR-0016
/// follow-up) the user-management/role-assignment surface deferred by RBAC v1. Scoped by
/// tenant so the same username can exist independently across tenants without collision.
#[async_trait]
pub trait LocalUserRepository: Send + Sync {
    async fn find_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<LocalUser>, LocalUserRepositoryError>;

    /// Tenant-unscoped by necessity: the MFA login challenge (ADR-0051) only knows a user id at
    /// that point in the flow, not which tenant typed it in -- `mfa_handler::post_mfa_challenge`
    /// gets the tenant back out of the challenge record itself and must cross-check it against
    /// what this returns before trusting the result.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<LocalUser>, LocalUserRepositoryError>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<LocalUser>, LocalUserRepositoryError>;

    async fn create(
        &self,
        user: LocalUser,
        actor: &str,
    ) -> Result<LocalUser, LocalUserRepositoryError>;

    async fn update_role(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        role: Role,
        actor: &str,
    ) -> Result<LocalUser, LocalUserRepositoryError>;

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), LocalUserRepositoryError>;

    /// The three MFA enrollment-state mutations (ADR-0051), kept on this trait rather than a
    /// separate one so a single in-memory test double can't diverge from the `LocalUser` it also
    /// serves reads from -- in Postgres both would hit the same `local_users` row regardless, but
    /// a fragmented pair of traits made that guarantee only accidental, not structural, for tests.
    /// None of these write an audit-log entry (a user managing their own second factor isn't an
    /// admin action on someone else, unlike `update_role`/`delete`).
    async fn set_pending_mfa_secret(
        &self,
        id: Uuid,
        secret_base32: &str,
    ) -> Result<(), LocalUserRepositoryError>;

    async fn confirm_mfa(&self, id: Uuid) -> Result<(), LocalUserRepositoryError>;

    async fn disable_mfa(&self, id: Uuid) -> Result<(), LocalUserRepositoryError>;
}

pub struct PostgresLocalUserRepository {
    pool: sqlx::PgPool,
}

impl PostgresLocalUserRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LocalUserRepository for PostgresLocalUserRepository {
    async fn find_by_username(
        &self,
        tenant_id: Uuid,
        username: &str,
    ) -> Result<Option<LocalUser>, LocalUserRepositoryError> {
        let row: Option<(Uuid, Uuid, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled FROM local_users WHERE tenant_id = $1 AND username = $2",
        )
        .bind(tenant_id)
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        row.map(|(id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled)| {
            let role: Role = role.parse().map_err(|e: common::ParseRoleError| {
                LocalUserRepositoryError::Backend(e.to_string())
            })?;
            Ok(LocalUser { id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled })
        })
        .transpose()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<LocalUser>, LocalUserRepositoryError> {
        let row: Option<(Uuid, Uuid, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled FROM local_users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        row.map(|(id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled)| {
            let role: Role = role.parse().map_err(|e: common::ParseRoleError| {
                LocalUserRepositoryError::Backend(e.to_string())
            })?;
            Ok(LocalUser { id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled })
        })
        .transpose()
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<LocalUser>, LocalUserRepositoryError> {
        let rows: Vec<(Uuid, Uuid, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled FROM local_users WHERE tenant_id = $1 ORDER BY username",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        rows.into_iter()
            .map(|(id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled)| {
                let role: Role = role.parse().map_err(|e: common::ParseRoleError| {
                    LocalUserRepositoryError::Backend(e.to_string())
                })?;
                Ok(LocalUser {
                    id,
                    tenant_id,
                    username,
                    password_hash,
                    role,
                    mfa_secret,
                    mfa_enabled,
                })
            })
            .collect()
    }

    async fn create(
        &self,
        user: LocalUser,
        actor: &str,
    ) -> Result<LocalUser, LocalUserRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        sqlx::query(
            "INSERT INTO local_users (id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(user.id)
        .bind(user.tenant_id)
        .bind(&user.username)
        .bind(&user.password_hash)
        .bind(user.role.to_string())
        .bind(&user.mfa_secret)
        .bind(user.mfa_enabled)
        .execute(&mut *tx)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: user.tenant_id,
                entity_type: "local_user".to_string(),
                entity_id: user.id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::to_value(&user).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(user)
    }

    async fn update_role(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        role: Role,
        actor: &str,
    ) -> Result<LocalUser, LocalUserRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        let existing: Option<(Uuid, Uuid, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled FROM local_users WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        let Some((id, tenant_id, username, password_hash, before_role, mfa_secret, mfa_enabled)) =
            existing
        else {
            return Err(LocalUserRepositoryError::NotFound(id));
        };
        let before_role: Role = before_role.parse().map_err(|e: common::ParseRoleError| {
            LocalUserRepositoryError::Backend(e.to_string())
        })?;
        let before = LocalUser {
            id,
            tenant_id,
            username: username.clone(),
            password_hash: password_hash.clone(),
            role: before_role,
            mfa_secret: mfa_secret.clone(),
            mfa_enabled,
        };

        sqlx::query("UPDATE local_users SET role = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(role.to_string())
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        let after =
            LocalUser { id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled };

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "local_user".to_string(),
                entity_id: id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::to_value(&after).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(after)
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        actor: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        let existing: Option<(Uuid, Uuid, String, String, String, Option<String>, bool)> = sqlx::query_as(
            "SELECT id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled FROM local_users WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        let Some((id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled)) =
            existing
        else {
            return Err(LocalUserRepositoryError::NotFound(id));
        };
        let role: Role = role.parse().map_err(|e: common::ParseRoleError| {
            LocalUserRepositoryError::Backend(e.to_string())
        })?;
        let before =
            LocalUser { id, tenant_id, username, password_hash, role, mfa_secret, mfa_enabled };

        sqlx::query("DELETE FROM local_users WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "local_user".to_string(),
                entity_id: id,
                change_type: ChangeType::Deleted,
                actor: actor.to_string(),
                before: Some(serde_json::to_value(&before).unwrap_or_default()),
                after: serde_json::Value::Null,
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;

        tx.commit().await.map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn set_pending_mfa_secret(
        &self,
        id: Uuid,
        secret_base32: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        sqlx::query("UPDATE local_users SET mfa_secret = $1, mfa_enabled = false WHERE id = $2")
            .bind(secret_base32)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn confirm_mfa(&self, id: Uuid) -> Result<(), LocalUserRepositoryError> {
        sqlx::query("UPDATE local_users SET mfa_enabled = true WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn disable_mfa(&self, id: Uuid) -> Result<(), LocalUserRepositoryError> {
        sqlx::query("UPDATE local_users SET mfa_secret = NULL, mfa_enabled = false WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
