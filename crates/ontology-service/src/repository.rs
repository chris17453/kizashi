use common::ontology::{ActionInvocation, Link, LinkType, Object, ObjectType};
use async_trait::async_trait;
use uuid::Uuid;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(String),
}

#[async_trait]
pub trait OntologyRepository: Send + Sync {
    async fn get_object_types(&self, tenant_id: Uuid) -> Result<Vec<ObjectType>, RepositoryError>;
    async fn upsert_object(&self, object: Object) -> Result<(), RepositoryError>;
    async fn get_object_type(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<ObjectType>, RepositoryError>;
    async fn list_link_types(&self, tenant_id: Uuid) -> Result<Vec<LinkType>, RepositoryError>;
    async fn list_objects(&self, tenant_id: Uuid, object_type_id: Option<Uuid>) -> Result<Vec<Object>, RepositoryError>;
    async fn get_object(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Object>, RepositoryError>;
    async fn traverse_links(&self, tenant_id: Uuid, source_object_id: Uuid, link_type_id: Uuid) -> Result<(Vec<Link>, Vec<Object>), RepositoryError>;
    async fn list_action_invocations(&self, tenant_id: Uuid) -> Result<Vec<ActionInvocation>, RepositoryError>;
    async fn insert_action_invocation(&self, invocation: ActionInvocation) -> Result<(), RepositoryError>;
}
