#[path = "role_test.rs"]
#[cfg(test)]
mod role_test;

use serde::{Deserialize, Serialize};

/// A user's permission level within a tenant (spec §8, "OpenShift-project-style" per-tenant
/// RBAC, ADR-0016). Ordered: `Viewer < Operator < Admin`. `Operator` and above may write
/// (create/update/delete) config entities; `Admin` is reserved for future
/// role-administration actions (assigning roles to other users). Stored as a single column on
/// `auth_service.local_users` — one role per user per tenant in v1, not a separate
/// role-assignment table (ADR-0016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    /// True if this role is at least as privileged as `min` — the standard write-path check
    /// (`role.at_least(Role::Operator)`).
    pub fn at_least(self, min: Role) -> bool {
        self >= min
    }
}

impl std::str::FromStr for Role {
    type Err = ParseRoleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "viewer" => Ok(Role::Viewer),
            "operator" => Ok(Role::Operator),
            "admin" => Ok(Role::Admin),
            other => Err(ParseRoleError(other.to_string())),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
            Role::Admin => "admin",
        };
        f.write_str(s)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown role: {0}")]
pub struct ParseRoleError(String);
