#[cfg(test)]
#[path = "workflow_repository_test.rs"]
mod workflow_repository_test;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::{WorkflowCase, WorkflowCaseKind, WorkflowCaseStatus};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum WorkflowRepositoryError {
    #[error("workflow case {0} was not found")]
    NotFound(Uuid),
    #[error("workflow case state transition is invalid")]
    InvalidTransition,
    #[error("workflow storage failed: {0}")]
    Backend(String),
}

#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    async fn open_approval(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        summary: &str,
        due_at: DateTime<Utc>,
    ) -> Result<WorkflowCase, WorkflowRepositoryError>;
    async fn open_reconciliation_failure(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        summary: &str,
    ) -> Result<WorkflowCase, WorkflowRepositoryError>;
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<WorkflowCase>, WorkflowRepositoryError>;
    async fn decide(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: WorkflowCaseStatus,
        actor: &str,
        note: Option<&str>,
    ) -> Result<WorkflowCase, WorkflowRepositoryError>;
}

pub struct PostgresWorkflowRepository {
    pool: sqlx::PgPool,
}
impl PostgresWorkflowRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type WorkflowRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    String,
    Option<DateTime<Utc>>,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);
fn status_name(value: WorkflowCaseStatus) -> Result<String, WorkflowRepositoryError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| WorkflowRepositoryError::Backend("invalid workflow status".to_string()))
}
fn kind_name(value: WorkflowCaseKind) -> Result<String, WorkflowRepositoryError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| WorkflowRepositoryError::Backend("invalid workflow kind".to_string()))
}
fn row_to_case(row: WorkflowRow) -> Result<WorkflowCase, WorkflowRepositoryError> {
    let (
        id,
        tenant_id,
        execution_id,
        kind,
        status,
        summary,
        due_at,
        decided_by,
        decision_note,
        created_at,
        resolved_at,
    ) = row;
    Ok(WorkflowCase {
        id,
        tenant_id,
        execution_id,
        kind: serde_json::from_value(serde_json::json!(kind))
            .map_err(|error| WorkflowRepositoryError::Backend(error.to_string()))?,
        status: serde_json::from_value(serde_json::json!(status))
            .map_err(|error| WorkflowRepositoryError::Backend(error.to_string()))?,
        summary,
        due_at,
        decided_by,
        decision_note,
        created_at,
        resolved_at,
    })
}
fn row_query() -> &'static str {
    "id,tenant_id,execution_id,kind,status,summary,due_at,decided_by,decision_note,created_at,resolved_at"
}
fn can_decide(status: WorkflowCaseStatus) -> bool {
    matches!(
        status,
        WorkflowCaseStatus::Approved | WorkflowCaseStatus::Rejected | WorkflowCaseStatus::Resolved
    )
}

#[async_trait]
impl WorkflowRepository for PostgresWorkflowRepository {
    async fn open_approval(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        summary: &str,
        due_at: DateTime<Utc>,
    ) -> Result<WorkflowCase, WorkflowRepositoryError> {
        let row: WorkflowRow = sqlx::query_as(&format!("INSERT INTO pipeline_workflow_cases (id,tenant_id,execution_id,kind,status,summary,due_at) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING {}", row_query()))
            .bind(Uuid::new_v4()).bind(tenant_id).bind(execution_id).bind(kind_name(WorkflowCaseKind::Approval)?).bind(status_name(WorkflowCaseStatus::PendingReview)?).bind(summary).bind(due_at).fetch_one(&self.pool).await.map_err(|error| WorkflowRepositoryError::Backend(error.to_string()))?;
        row_to_case(row)
    }
    async fn open_reconciliation_failure(
        &self,
        tenant_id: Uuid,
        execution_id: Uuid,
        source_event_id: &str,
        summary: &str,
    ) -> Result<WorkflowCase, WorkflowRepositoryError> {
        let due_at = Utc::now() + chrono::Duration::hours(24);
        let row: WorkflowRow = sqlx::query_as(&format!("INSERT INTO pipeline_workflow_cases (id,tenant_id,execution_id,source_event_id,kind,status,summary,due_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id,execution_id,source_event_id,kind) DO UPDATE SET summary=EXCLUDED.summary,due_at=EXCLUDED.due_at,status='exception',resolved_at=NULL RETURNING {}", row_query()))
            .bind(Uuid::new_v4()).bind(tenant_id).bind(execution_id).bind(source_event_id).bind(kind_name(WorkflowCaseKind::ReconciliationFailure)?).bind(status_name(WorkflowCaseStatus::Exception)?).bind(summary).bind(due_at).fetch_one(&self.pool).await.map_err(|error| WorkflowRepositoryError::Backend(error.to_string()))?;
        row_to_case(row)
    }
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<WorkflowCase>, WorkflowRepositoryError> {
        let rows: Vec<WorkflowRow> = sqlx::query_as(&format!("SELECT {} FROM pipeline_workflow_cases WHERE tenant_id=$1 ORDER BY resolved_at NULLS FIRST,due_at NULLS LAST,created_at DESC", row_query())).bind(tenant_id).fetch_all(&self.pool).await.map_err(|error| WorkflowRepositoryError::Backend(error.to_string()))?;
        rows.into_iter().map(row_to_case).collect()
    }
    async fn decide(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: WorkflowCaseStatus,
        actor: &str,
        note: Option<&str>,
    ) -> Result<WorkflowCase, WorkflowRepositoryError> {
        if !can_decide(status) {
            return Err(WorkflowRepositoryError::InvalidTransition);
        }
        let row: Option<WorkflowRow> = sqlx::query_as(&format!("UPDATE pipeline_workflow_cases SET status=$1,decided_by=$2,decision_note=$3,resolved_at=now() WHERE tenant_id=$4 AND id=$5 AND resolved_at IS NULL RETURNING {}", row_query())).bind(status_name(status)?).bind(actor).bind(note).bind(tenant_id).bind(id).fetch_optional(&self.pool).await.map_err(|error| WorkflowRepositoryError::Backend(error.to_string()))?;
        row.map(row_to_case).transpose()?.ok_or(WorkflowRepositoryError::NotFound(id))
    }
}
