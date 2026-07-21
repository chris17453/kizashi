#[path = "trigger_repository_test.rs"]
#[cfg(test)]
pub(crate) mod trigger_repository_test;

use async_trait::async_trait;
use common::TriggerDefinition;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TriggerRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Looks up enabled TriggerDefinitions matching a tenant/event_type (spec §5.4). v1 owns this
/// data directly in Trigger Engine's own Postgres schema rather than depending on Config/Admin
/// Service (not yet built), same interim-ownership pattern as Normalization Service's
/// NormalizationMapping repository.
#[async_trait]
pub trait TriggerRepository: Send + Sync {
    async fn active_triggers_for(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Vec<TriggerDefinition>, TriggerRepositoryError>;

    /// Looks up a single trigger by id, regardless of enabled/disabled — used by the
    /// `GET /v1/triggers/:id` API so Action Executor can resolve which actions to run for a
    /// firing event without reading Trigger Engine's database directly (spec §2 principle 1).
    async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerRepositoryError>;

    /// Inserts or replaces a trigger by id (ADR-0018): the write side of syncing Trigger
    /// Engine's own copy of trigger definitions with what config-admin-service's `trigger.
    /// changed` messages say is current.
    async fn upsert(&self, trigger: TriggerDefinition) -> Result<(), TriggerRepositoryError>;

    /// Removes a trigger by id (ADR-0109): the write side of syncing a `TriggerChangeEvent::
    /// Deleted` message, same shape as `SensorRepository::delete` in agent-scheduler.
    async fn delete(&self, id: Uuid) -> Result<(), TriggerRepositoryError>;
}

pub struct PostgresTriggerRepository {
    pool: sqlx::PgPool,
}

impl PostgresTriggerRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type TriggerRow = (
    Uuid,
    Uuid,
    String,
    String,
    sqlx::types::Json<common::TriggerCondition>,
    i64,
    sqlx::types::Json<Vec<common::ActionRef>>,
    bool,
);

fn row_to_trigger(row: TriggerRow) -> TriggerDefinition {
    let (id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled) = row;
    TriggerDefinition {
        id,
        tenant_id,
        name,
        event_type_match,
        condition: condition.0,
        window_seconds,
        actions: actions.0,
        enabled,
    }
}

#[async_trait]
impl TriggerRepository for PostgresTriggerRepository {
    async fn active_triggers_for(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Vec<TriggerDefinition>, TriggerRepositoryError> {
        // ADR-0027: a trigger matches either by the plain `event_type_match` column (the two
        // single-event-type shapes), or — for CorrelatedOverWindow triggers — if `event_type`
        // appears anywhere in its `condition`'s `conditions` array, checked via JSONB
        // containment against the same `condition` column already stored, no schema change.
        let containment_probe = serde_json::json!([{"event_type": event_type}]);
        let rows: Vec<TriggerRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled
            FROM trigger_definitions
            WHERE tenant_id = $1
              AND enabled = true
              AND (
                event_type_match = $2
                OR condition -> 'conditions' @> $3::jsonb
              )
            "#,
        )
        .bind(tenant_id)
        .bind(event_type)
        .bind(containment_probe)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| TriggerRepositoryError::Backend(e.to_string()))?;

        Ok(rows.into_iter().map(row_to_trigger).collect())
    }

    async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<TriggerDefinition>, TriggerRepositoryError> {
        let row: Option<TriggerRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled
            FROM trigger_definitions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| TriggerRepositoryError::Backend(e.to_string()))?;

        Ok(row.map(row_to_trigger))
    }

    async fn upsert(&self, trigger: TriggerDefinition) -> Result<(), TriggerRepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO trigger_definitions
                (id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                name = EXCLUDED.name,
                event_type_match = EXCLUDED.event_type_match,
                condition = EXCLUDED.condition,
                window_seconds = EXCLUDED.window_seconds,
                actions = EXCLUDED.actions,
                enabled = EXCLUDED.enabled
            "#,
        )
        .bind(trigger.id)
        .bind(trigger.tenant_id)
        .bind(trigger.name)
        .bind(trigger.event_type_match)
        .bind(sqlx::types::Json(trigger.condition))
        .bind(trigger.window_seconds)
        .bind(sqlx::types::Json(trigger.actions))
        .bind(trigger.enabled)
        .execute(&self.pool)
        .await
        .map_err(|e| TriggerRepositoryError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), TriggerRepositoryError> {
        sqlx::query("DELETE FROM trigger_definitions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| TriggerRepositoryError::Backend(e.to_string()))?;
        Ok(())
    }
}
