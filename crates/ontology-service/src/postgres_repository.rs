use crate::repository::{OntologyRepository, RepositoryError};
use async_trait::async_trait;
use common::ontology::{
    ActionInvocation, ActionReview, ActionType, ActionTypeHistory, Link, LinkType, Object,
    ObjectHistory, ObjectType,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresOntologyRepository {
    pool: PgPool,
}

impl PostgresOntologyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OntologyRepository for PostgresOntologyRepository {
    async fn get_object_types(&self, tenant_id: Uuid) -> Result<Vec<ObjectType>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, name, version, property_schema, mapping_rules, created_at, updated_at FROM object_types WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn create_object_type(&self, t: ObjectType) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO object_types (id, tenant_id, name, version, property_schema, mapping_rules, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(t.id).bind(t.tenant_id).bind(t.name).bind(t.version).bind(t.property_schema).bind(t.mapping_rules).bind(t.created_at).bind(t.updated_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn update_object_type(&self, t: ObjectType) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE object_types SET name=$3, version=$4, property_schema=$5, mapping_rules=$6, updated_at=$8 WHERE id=$1 AND tenant_id=$2")
            .bind(t.id).bind(t.tenant_id).bind(t.name).bind(t.version).bind(t.property_schema).bind(t.mapping_rules).bind(t.created_at).bind(t.updated_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn delete_object_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM object_types WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
            .map(|_| ())
    }

    async fn upsert_object(&self, object: Object) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO objects (id, tenant_id, object_type_id, properties, source_lineage, updated_at) 
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (id) DO UPDATE SET 
                properties = objects.properties || EXCLUDED.properties,
                source_lineage = objects.source_lineage || EXCLUDED.source_lineage,
                updated_at = NOW()
            "#
        )
        .bind(object.id)
        .bind(object.tenant_id)
        .bind(object.object_type_id)
        .bind(object.properties)
        .bind(object.source_lineage)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn create_object(&self, object: Object) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO objects (id, tenant_id, object_type_id, properties, source_lineage, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(object.id).bind(object.tenant_id).bind(object.object_type_id)
        .bind(object.properties).bind(object.source_lineage).bind(object.created_at).bind(object.updated_at)
        .execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn update_object(&self, object: Object) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE objects SET object_type_id = $3, properties = $4, source_lineage = $5, updated_at = $6 WHERE id = $1 AND tenant_id = $2",
        )
        .bind(object.id).bind(object.tenant_id).bind(object.object_type_id)
        .bind(object.properties).bind(object.source_lineage).bind(object.updated_at)
        .execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|result| {
            if result.rows_affected() == 0 { () } else { () }
        })
    }

    async fn delete_object(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM objects WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
            .map(|_| ())
    }

    async fn list_object_history(
        &self,
        tenant_id: Uuid,
        object_id: Uuid,
    ) -> Result<Vec<ObjectHistory>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, object_id, change_type, actor, before_state, after_state, changed_at FROM object_history WHERE tenant_id=$1 AND object_id=$2 ORDER BY changed_at DESC",
        )
        .bind(tenant_id)
        .bind(object_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn record_object_history(&self, history: ObjectHistory) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO object_history (id, tenant_id, object_id, change_type, actor, before_state, after_state, changed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(history.id)
        .bind(history.tenant_id)
        .bind(history.object_id)
        .bind(history.change_type)
        .bind(history.actor)
        .bind(history.before_state)
        .bind(history.after_state)
        .bind(history.changed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
        .map(|_| ())
    }

    async fn get_object_type(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ObjectType>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, name, version, property_schema, mapping_rules, created_at, updated_at FROM object_types WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn list_link_types(&self, tenant_id: Uuid) -> Result<Vec<LinkType>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, name, source_object_type_id, target_object_type_id, cardinality, properties_schema, created_at, updated_at FROM link_types WHERE tenant_id = $1"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn list_links(&self, tenant_id: Uuid) -> Result<Vec<Link>, RepositoryError> {
        sqlx::query_as("SELECT id, tenant_id, link_type_id, source_object_id, target_object_id, properties, created_at, updated_at FROM links WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 500")
            .bind(tenant_id).fetch_all(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn create_link(&self, link: Link) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO links (id, tenant_id, link_type_id, source_object_id, target_object_id, properties, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(link.id).bind(link.tenant_id).bind(link.link_type_id).bind(link.source_object_id).bind(link.target_object_id).bind(link.properties).bind(link.created_at).bind(link.updated_at)
            .execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn update_link(&self, link: Link) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE links SET link_type_id=$3, source_object_id=$4, target_object_id=$5, properties=$6, updated_at=$7 WHERE id=$1 AND tenant_id=$2")
            .bind(link.id).bind(link.tenant_id).bind(link.link_type_id).bind(link.source_object_id).bind(link.target_object_id).bind(link.properties).bind(link.updated_at)
            .execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn delete_link(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM links WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
            .map(|_| ())
    }

    async fn create_link_type(&self, l: LinkType) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO link_types (id, tenant_id, name, source_object_type_id, target_object_type_id, cardinality, properties_schema, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(l.id).bind(l.tenant_id).bind(l.name).bind(l.source_object_type_id).bind(l.target_object_type_id).bind(l.cardinality).bind(l.properties_schema).bind(l.created_at).bind(l.updated_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn update_link_type(&self, l: LinkType) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE link_types SET name=$3, source_object_type_id=$4, target_object_type_id=$5, cardinality=$6, properties_schema=$7, updated_at=$8 WHERE id=$1 AND tenant_id=$2")
            .bind(l.id).bind(l.tenant_id).bind(l.name).bind(l.source_object_type_id).bind(l.target_object_type_id).bind(l.cardinality).bind(l.properties_schema).bind(l.updated_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn delete_link_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM link_types WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
            .map(|_| ())
    }

    async fn list_objects(
        &self,
        tenant_id: Uuid,
        object_type_id: Option<Uuid>,
    ) -> Result<Vec<Object>, RepositoryError> {
        if let Some(type_id) = object_type_id {
            sqlx::query_as(
                "SELECT id, tenant_id, object_type_id, properties, source_lineage, created_at, updated_at FROM objects WHERE tenant_id = $1 AND object_type_id = $2 LIMIT 100"
            )
            .bind(tenant_id)
            .bind(type_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
        } else {
            sqlx::query_as(
                "SELECT id, tenant_id, object_type_id, properties, source_lineage, created_at, updated_at FROM objects WHERE tenant_id = $1 LIMIT 100"
            )
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
        }
    }

    async fn get_object(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Object>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, object_type_id, properties, source_lineage, created_at, updated_at FROM objects WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn traverse_links(
        &self,
        tenant_id: Uuid,
        source_object_id: Uuid,
        link_type_id: Uuid,
    ) -> Result<(Vec<Link>, Vec<Object>), RepositoryError> {
        let links: Vec<Link> = sqlx::query_as(
            "SELECT id, tenant_id, link_type_id, source_object_id, target_object_id, properties, created_at, updated_at FROM links WHERE tenant_id = $1 AND link_type_id = $2 AND source_object_id = $3"
        )
        .bind(tenant_id)
        .bind(link_type_id)
        .bind(source_object_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut targets = Vec::new();
        for link in &links {
            if let Ok(Some(obj)) = sqlx::query_as::<_, Object>(
                "SELECT id, tenant_id, object_type_id, properties, source_lineage, created_at, updated_at FROM objects WHERE id = $1 AND tenant_id = $2"
            )
            .bind(link.target_object_id)
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await {
                targets.push(obj);
            }
        }

        Ok((links, targets))
    }

    async fn list_action_invocations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionInvocation>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, contract_snapshot, executed_at FROM action_invocations WHERE tenant_id = $1 ORDER BY executed_at DESC LIMIT 100"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn list_action_types(&self, tenant_id: Uuid) -> Result<Vec<ActionType>, RepositoryError> {
        sqlx::query_as("SELECT id, tenant_id, name, target_object_type_id, parameter_schema, preconditions, effect_definition, created_at, updated_at FROM action_types WHERE tenant_id=$1 ORDER BY name")
            .bind(tenant_id).fetch_all(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn create_action_type(&self, a: ActionType) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO action_types (id, tenant_id, name, target_object_type_id, parameter_schema, preconditions, effect_definition, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(a.id).bind(a.tenant_id).bind(a.name).bind(a.target_object_type_id).bind(a.parameter_schema).bind(a.preconditions).bind(a.effect_definition).bind(a.created_at).bind(a.updated_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn update_action_type(&self, a: ActionType) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE action_types SET name=$3, target_object_type_id=$4, parameter_schema=$5, preconditions=$6, effect_definition=$7, updated_at=$8 WHERE id=$1 AND tenant_id=$2")
            .bind(a.id).bind(a.tenant_id).bind(a.name).bind(a.target_object_type_id).bind(a.parameter_schema).bind(a.preconditions).bind(a.effect_definition).bind(a.updated_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn delete_action_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM action_types WHERE id=$1 AND tenant_id=$2")
            .bind(id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
            .map(|_| ())
    }

    async fn list_action_type_history(
        &self,
        tenant_id: Uuid,
        action_type_id: Uuid,
    ) -> Result<Vec<ActionTypeHistory>, RepositoryError> {
        sqlx::query_as("SELECT id, tenant_id, action_type_id, change_type, actor, before_state, after_state, changed_at FROM action_type_history WHERE tenant_id=$1 AND action_type_id=$2 ORDER BY changed_at DESC")
            .bind(tenant_id).bind(action_type_id).fetch_all(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn record_action_type_history(
        &self,
        history: ActionTypeHistory,
    ) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO action_type_history (id, tenant_id, action_type_id, change_type, actor, before_state, after_state, changed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(history.id).bind(history.tenant_id).bind(history.action_type_id).bind(history.change_type).bind(history.actor).bind(history.before_state).bind(history.after_state).bind(history.changed_at).execute(&self.pool).await.map_err(|e| RepositoryError::Database(e.to_string())).map(|_| ())
    }

    async fn insert_action_invocation(
        &self,
        payload: ActionInvocation,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO action_invocations (id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, contract_snapshot, executed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(payload.id)
        .bind(payload.tenant_id)
        .bind(payload.action_type_id)
        .bind(payload.target_object_ids)
        .bind(payload.parameters)
        .bind(payload.outcome)
        .bind(payload.triggering_event_ref)
        .bind(payload.contract_snapshot)
        .bind(payload.executed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_action_reviews(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionReview>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, invocation_id, status, assignee, note, reviewed_by, due_at, created_at, updated_at FROM action_reviews WHERE tenant_id = $1 ORDER BY updated_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn upsert_action_review(&self, review: ActionReview) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO action_reviews (id, tenant_id, invocation_id, status, assignee, note, reviewed_by, due_at, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (tenant_id, invocation_id) DO UPDATE SET status=excluded.status, assignee=excluded.assignee, note=excluded.note, reviewed_by=excluded.reviewed_by, due_at=excluded.due_at, updated_at=excluded.updated_at",
        )
        .bind(review.id)
        .bind(review.tenant_id)
        .bind(review.invocation_id)
        .bind(review.status)
        .bind(review.assignee)
        .bind(review.note)
        .bind(review.reviewed_by)
        .bind(review.due_at)
        .bind(review.created_at)
        .bind(review.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
        .map(|_| ())
    }
}
