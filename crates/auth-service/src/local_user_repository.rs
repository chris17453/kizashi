#[path = "local_user_repository_test.rs"]
#[cfg(test)]
pub(crate) mod local_user_repository_test;

use crate::audit_log::{record_audit_entry, AuditLogEntry, ChangeType};
use async_trait::async_trait;
use common::Role;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceAccount {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub label: String,
    pub role: Role,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

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

    /// Self-service password change (ADR-0057, closing a gap ADR-0052 explicitly flagged: the
    /// only prior way to change a password was an admin delete+recreate). No audit-log entry,
    /// same reasoning as the MFA mutations above -- a user changing their own password isn't an
    /// admin action on someone else.
    async fn update_password(
        &self,
        id: Uuid,
        new_password_hash: &str,
    ) -> Result<(), LocalUserRepositoryError>;

    async fn list_service_accounts(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("service accounts unavailable".to_string()))
    }

    async fn create_service_account(
        &self,
        _account: ServiceAccount,
        _token_hash: &str,
        _actor: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("service accounts unavailable".to_string()))
    }

    async fn revoke_service_account(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _actor: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("service accounts unavailable".to_string()))
    }

    async fn find_service_account_by_token_hash(
        &self,
        _token_hash: &str,
    ) -> Result<Option<ServiceAccount>, LocalUserRepositoryError> {
        Err(LocalUserRepositoryError::Backend("service accounts unavailable".to_string()))
    }
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

    async fn update_password(
        &self,
        id: Uuid,
        new_password_hash: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        sqlx::query("UPDATE local_users SET password_hash = $1 WHERE id = $2")
            .bind(new_password_hash)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn list_service_accounts(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, LocalUserRepositoryError> {
        let rows: Vec<(Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            "SELECT id, tenant_id, label, role, created_at, revoked_at FROM service_accounts WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        rows.into_iter()
            .map(|(id, tenant_id, label, role, created_at, revoked_at)| {
                Ok(ServiceAccount {
                    id,
                    tenant_id,
                    label,
                    role: role.parse().map_err(|e: common::ParseRoleError| {
                        LocalUserRepositoryError::Backend(e.to_string())
                    })?,
                    created_at,
                    revoked_at,
                })
            })
            .collect()
    }

    async fn create_service_account(
        &self,
        account: ServiceAccount,
        token_hash: &str,
        actor: &str,
    ) -> Result<(), LocalUserRepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        sqlx::query("INSERT INTO service_accounts (id, tenant_id, label, role, token_hash, created_at, revoked_at) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(account.id).bind(account.tenant_id).bind(&account.label).bind(account.role.to_string())
            .bind(token_hash).bind(account.created_at).bind(account.revoked_at).execute(&mut *tx).await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id: account.tenant_id,
                entity_type: "service_account".to_string(),
                entity_id: account.id,
                change_type: ChangeType::Created,
                actor: actor.to_string(),
                before: None,
                after: serde_json::to_value(&account).unwrap_or_default(),
                changed_at: account.created_at,
            },
        )
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn revoke_service_account(
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
        let updated: Option<(Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            "UPDATE service_accounts SET revoked_at = COALESCE(revoked_at, NOW()) WHERE id = $1 AND tenant_id = $2 RETURNING id, tenant_id, label, role, created_at, revoked_at",
        ).bind(id).bind(tenant_id).fetch_optional(&mut *tx).await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        let Some((id, tenant_id, label, role, created_at, revoked_at)) = updated else {
            return Err(LocalUserRepositoryError::NotFound(id));
        };
        let account = ServiceAccount {
            id,
            tenant_id,
            label,
            role: role.parse().map_err(|e: common::ParseRoleError| {
                LocalUserRepositoryError::Backend(e.to_string())
            })?,
            created_at,
            revoked_at,
        };
        record_audit_entry(
            &mut tx,
            &AuditLogEntry {
                id: Uuid::new_v4(),
                tenant_id,
                entity_type: "service_account".to_string(),
                entity_id: id,
                change_type: ChangeType::Updated,
                actor: actor.to_string(),
                before: None,
                after: serde_json::to_value(&account).unwrap_or_default(),
                changed_at: chrono::Utc::now(),
            },
        )
        .await
        .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        tx.commit().await.map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn find_service_account_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<ServiceAccount>, LocalUserRepositoryError> {
        let row: Option<(Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            "SELECT id, tenant_id, label, role, created_at, revoked_at FROM service_accounts WHERE token_hash = $1",
        ).bind(token_hash).fetch_optional(&self.pool).await
            .map_err(|e| LocalUserRepositoryError::Backend(e.to_string()))?;
        row.map(|(id, tenant_id, label, role, created_at, revoked_at)| {
            Ok(ServiceAccount {
                id,
                tenant_id,
                label,
                role: role.parse().map_err(|e: common::ParseRoleError| {
                    LocalUserRepositoryError::Backend(e.to_string())
                })?,
                created_at,
                revoked_at,
            })
        })
        .transpose()
    }
}
