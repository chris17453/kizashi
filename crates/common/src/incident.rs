#[path = "incident_test.rs"]
#[cfg(test)]
mod incident_test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Groups related `Event`s into one trackable problem (ADR-0111) — the single biggest gap
/// identified against Keep's Incidents feature. v1 is manual-only: an operator selects Events
/// on the Events page and creates an Incident from them; auto-correlation, dedup, and
/// AI-generated summaries are deferred to follow-up ADRs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Incident {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub title: String,
    pub summary: String,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    #[serde(default)]
    pub assigned_to: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct IncidentNote {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub incident_id: Uuid,
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::str::FromStr for IncidentSeverity {
    type Err = ParseIncidentFieldError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(IncidentSeverity::Low),
            "medium" => Ok(IncidentSeverity::Medium),
            "high" => Ok(IncidentSeverity::High),
            "critical" => Ok(IncidentSeverity::Critical),
            other => Err(ParseIncidentFieldError(other.to_string())),
        }
    }
}

impl std::fmt::Display for IncidentSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            IncidentSeverity::Low => "low",
            IncidentSeverity::Medium => "medium",
            IncidentSeverity::High => "high",
            IncidentSeverity::Critical => "critical",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Acknowledged,
    Resolved,
}

impl std::str::FromStr for IncidentStatus {
    type Err = ParseIncidentFieldError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(IncidentStatus::Open),
            "acknowledged" => Ok(IncidentStatus::Acknowledged),
            "resolved" => Ok(IncidentStatus::Resolved),
            other => Err(ParseIncidentFieldError(other.to_string())),
        }
    }
}

impl std::fmt::Display for IncidentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            IncidentStatus::Open => "open",
            IncidentStatus::Acknowledged => "acknowledged",
            IncidentStatus::Resolved => "resolved",
        };
        f.write_str(s)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown incident field value: {0}")]
pub struct ParseIncidentFieldError(String);
