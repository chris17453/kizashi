use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable execution record for a recurring report definition. The artifact URL is intentionally
/// a Console route, so the report remains tenant/auth scoped rather than leaking a raw storage
/// object to an email recipient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportRun {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub schedule_id: Uuid,
    pub schedule_name: String,
    pub recipient: String,
    pub format: String,
    pub status: String,
    pub error: Option<String>,
    pub artifact_url: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl ReportRun {
    pub fn new(
        tenant_id: Uuid,
        schedule_id: Uuid,
        schedule_name: impl Into<String>,
        recipient: impl Into<String>,
        artifact_url: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            schedule_id,
            schedule_name: schedule_name.into(),
            recipient: recipient.into(),
            format: "csv".into(),
            status: "running".into(),
            error: None,
            artifact_url: Some(artifact_url.into()),
            started_at: Utc::now(),
            completed_at: None,
        }
    }
}
