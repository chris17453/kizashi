use async_trait::async_trait;
use common::ontology::{
    ActionInvocation, ActionReview, ActionType, ActionTypeHistory, Link, LinkType, Object,
    ObjectHistory, ObjectType,
};
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
    async fn create_object_type(&self, object_type: ObjectType) -> Result<(), RepositoryError>;
    async fn update_object_type(&self, object_type: ObjectType) -> Result<(), RepositoryError>;
    async fn delete_object_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError>;
    async fn upsert_object(&self, object: Object) -> Result<(), RepositoryError>;
    async fn create_object(&self, object: Object) -> Result<(), RepositoryError>;
    async fn update_object(&self, object: Object) -> Result<(), RepositoryError>;
    async fn delete_object(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError>;
    async fn list_object_history(
        &self,
        tenant_id: Uuid,
        object_id: Uuid,
    ) -> Result<Vec<ObjectHistory>, RepositoryError>;
    async fn record_object_history(&self, history: ObjectHistory) -> Result<(), RepositoryError>;
    async fn get_object_type(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ObjectType>, RepositoryError>;
    async fn list_link_types(&self, tenant_id: Uuid) -> Result<Vec<LinkType>, RepositoryError>;
    async fn list_links(&self, tenant_id: Uuid) -> Result<Vec<Link>, RepositoryError>;
    async fn create_link(&self, link: Link) -> Result<(), RepositoryError>;
    async fn update_link(&self, link: Link) -> Result<(), RepositoryError>;
    async fn delete_link(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError>;
    async fn create_link_type(&self, link_type: LinkType) -> Result<(), RepositoryError>;
    async fn update_link_type(&self, link_type: LinkType) -> Result<(), RepositoryError>;
    async fn delete_link_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError>;
    async fn list_objects(
        &self,
        tenant_id: Uuid,
        object_type_id: Option<Uuid>,
    ) -> Result<Vec<Object>, RepositoryError>;
    async fn get_object(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Object>, RepositoryError>;
    async fn traverse_links(
        &self,
        tenant_id: Uuid,
        source_object_id: Uuid,
        link_type_id: Uuid,
    ) -> Result<(Vec<Link>, Vec<Object>), RepositoryError>;
    async fn list_action_invocations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionInvocation>, RepositoryError>;
    async fn list_action_types(&self, tenant_id: Uuid) -> Result<Vec<ActionType>, RepositoryError>;
    async fn create_action_type(&self, action_type: ActionType) -> Result<(), RepositoryError>;
    async fn update_action_type(&self, action_type: ActionType) -> Result<(), RepositoryError>;
    async fn delete_action_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError>;
    async fn list_action_type_history(
        &self,
        tenant_id: Uuid,
        action_type_id: Uuid,
    ) -> Result<Vec<ActionTypeHistory>, RepositoryError>;
    async fn record_action_type_history(
        &self,
        history: ActionTypeHistory,
    ) -> Result<(), RepositoryError>;
    async fn insert_action_invocation(
        &self,
        invocation: ActionInvocation,
    ) -> Result<(), RepositoryError>;
    async fn list_action_reviews(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionReview>, RepositoryError>;
    async fn upsert_action_review(&self, review: ActionReview) -> Result<(), RepositoryError>;
}
