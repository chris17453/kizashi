#[path = "saved_search_query_repository_test.rs"]
#[cfg(test)]
pub(crate) mod saved_search_query_repository_test;

use async_trait::async_trait;
use common::SavedSearchQuery;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SavedSearchQueryRepositoryError {
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("no saved search query with id {0}")]
    NotFound(Uuid),
}

/// CRUD for `SavedSearchQuery` (ADR-0029). Unlike every other entity in this service, writes
/// here are **not** audit-logged — a saved search is a personal/team UI bookmark with zero
/// effect on the ingestion/normalization/analysis/trigger pipeline, not admin/config in the
/// CLAUDE.md §5 sense.
#[async_trait]
pub trait SavedSearchQueryRepository: Send + Sync {
    async fn create(
        &self,
        query: SavedSearchQuery,
    ) -> Result<SavedSearchQuery, SavedSearchQueryRepositoryError>;

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueryRepositoryError>;

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), SavedSearchQueryRepositoryError>;
}

pub struct PostgresSavedSearchQueryRepository {
    pool: sqlx::PgPool,
}

impl PostgresSavedSearchQueryRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

type SavedSearchQueryRow = (Uuid, Uuid, String, serde_json::Value);

fn row_to_query(row: SavedSearchQueryRow) -> SavedSearchQuery {
    let (id, tenant_id, name, filter) = row;
    SavedSearchQuery { id, tenant_id, name, filter }
}

#[async_trait]
impl SavedSearchQueryRepository for PostgresSavedSearchQueryRepository {
    async fn create(
        &self,
        query: SavedSearchQuery,
    ) -> Result<SavedSearchQuery, SavedSearchQueryRepositoryError> {
        sqlx::query(
            "INSERT INTO saved_search_queries (id, tenant_id, name, filter) VALUES ($1, $2, $3, $4)",
        )
        .bind(query.id)
        .bind(query.tenant_id)
        .bind(&query.name)
        .bind(&query.filter)
        .execute(&self.pool)
        .await
        .map_err(|e| SavedSearchQueryRepositoryError::Backend(e.to_string()))?;
        Ok(query)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SavedSearchQuery>, SavedSearchQueryRepositoryError> {
        let rows: Vec<SavedSearchQueryRow> = sqlx::query_as(
            "SELECT id, tenant_id, name, filter FROM saved_search_queries WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SavedSearchQueryRepositoryError::Backend(e.to_string()))?;
        Ok(rows.into_iter().map(row_to_query).collect())
    }

    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<(), SavedSearchQueryRepositoryError> {
        let result =
            sqlx::query("DELETE FROM saved_search_queries WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(|e| SavedSearchQueryRepositoryError::Backend(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(SavedSearchQueryRepositoryError::NotFound(id));
        }
        Ok(())
    }
}
