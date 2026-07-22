use crate::repository::{OntologyRepository, RepositoryError};
use async_trait::async_trait;
use common::ontology::{ActionInvocation, Link, LinkType, Object, ObjectType};
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

    async fn get_object_type(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<ObjectType>, RepositoryError> {
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

    async fn list_objects(&self, tenant_id: Uuid, object_type_id: Option<Uuid>) -> Result<Vec<Object>, RepositoryError> {
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

    async fn get_object(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Object>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, object_type_id, properties, source_lineage, created_at, updated_at FROM objects WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn traverse_links(&self, tenant_id: Uuid, source_object_id: Uuid, link_type_id: Uuid) -> Result<(Vec<Link>, Vec<Object>), RepositoryError> {
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

    async fn list_action_invocations(&self, tenant_id: Uuid) -> Result<Vec<ActionInvocation>, RepositoryError> {
        sqlx::query_as(
            "SELECT id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, executed_at FROM action_invocations WHERE tenant_id = $1 ORDER BY executed_at DESC LIMIT 100"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))
    }

    async fn insert_action_invocation(&self, payload: ActionInvocation) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO action_invocations (id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, executed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(payload.id)
        .bind(payload.tenant_id)
        .bind(payload.action_type_id)
        .bind(payload.target_object_ids)
        .bind(payload.parameters)
        .bind(payload.outcome)
        .bind(payload.triggering_event_ref)
        .bind(payload.executed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}
