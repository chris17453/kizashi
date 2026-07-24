use crate::repository::{OntologyRepository, RepositoryError};
use async_trait::async_trait;
use common::ontology::{
    ActionInvocation, ActionReview, ActionType, ActionTypeHistory, Link, LinkType, Object,
    ObjectHistory, ObjectType,
};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default, Clone)]
pub struct InMemoryOntologyRepository {
    pub object_types: Arc<Mutex<Vec<ObjectType>>>,
    pub objects: Arc<Mutex<Vec<Object>>>,
    pub object_history: Arc<Mutex<Vec<ObjectHistory>>>,
    pub link_types: Arc<Mutex<Vec<LinkType>>>,
    pub links: Arc<Mutex<Vec<Link>>>,
    pub action_invocations: Arc<Mutex<Vec<ActionInvocation>>>,
    pub action_types: Arc<Mutex<Vec<ActionType>>>,
    pub action_reviews: Arc<Mutex<Vec<ActionReview>>>,
    pub action_type_history: Arc<Mutex<Vec<ActionTypeHistory>>>,
}

impl InMemoryOntologyRepository {
    pub fn new() -> Self {
        Self {
            object_types: Arc::new(Mutex::new(Vec::new())),
            objects: Arc::new(Mutex::new(Vec::new())),
            object_history: Arc::new(Mutex::new(Vec::new())),
            link_types: Arc::new(Mutex::new(Vec::new())),
            links: Arc::new(Mutex::new(Vec::new())),
            action_invocations: Arc::new(Mutex::new(Vec::new())),
            action_types: Arc::new(Mutex::new(Vec::new())),
            action_reviews: Arc::new(Mutex::new(Vec::new())),
            action_type_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn create_object_type(&self, object_type: ObjectType) -> Result<(), RepositoryError> {
        self.object_types.lock().unwrap().push(object_type);
        Ok(())
    }

    pub async fn get_objects(&self, tenant_id: Uuid) -> Result<Vec<Object>, RepositoryError> {
        let objects = self.objects.lock().unwrap();
        Ok(objects.iter().filter(|o| o.tenant_id == tenant_id).cloned().collect())
    }
}

#[async_trait]
impl OntologyRepository for InMemoryOntologyRepository {
    async fn get_object_types(&self, tenant_id: Uuid) -> Result<Vec<ObjectType>, RepositoryError> {
        let types = self.object_types.lock().unwrap();
        Ok(types.iter().filter(|t| t.tenant_id == tenant_id).cloned().collect())
    }

    async fn create_object_type(&self, object_type: ObjectType) -> Result<(), RepositoryError> {
        self.object_types.lock().unwrap().push(object_type);
        Ok(())
    }
    async fn update_object_type(&self, object_type: ObjectType) -> Result<(), RepositoryError> {
        let mut types = self.object_types.lock().unwrap();
        if let Some(pos) = types
            .iter()
            .position(|t| t.id == object_type.id && t.tenant_id == object_type.tenant_id)
        {
            types[pos] = object_type;
        }
        Ok(())
    }
    async fn delete_object_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        self.object_types.lock().unwrap().retain(|t| !(t.id == id && t.tenant_id == tenant_id));
        Ok(())
    }

    async fn upsert_object(&self, object: Object) -> Result<(), RepositoryError> {
        let mut objects = self.objects.lock().unwrap();
        if let Some(pos) = objects.iter().position(|o| o.id == object.id) {
            objects[pos] = object;
        } else {
            objects.push(object);
        }
        Ok(())
    }
    async fn create_object(&self, object: Object) -> Result<(), RepositoryError> {
        let mut objects = self.objects.lock().unwrap();
        if objects
            .iter()
            .any(|existing| existing.id == object.id && existing.tenant_id == object.tenant_id)
        {
            return Err(RepositoryError::Database("object already exists".to_string()));
        }
        objects.push(object);
        Ok(())
    }
    async fn update_object(&self, object: Object) -> Result<(), RepositoryError> {
        let mut objects = self.objects.lock().unwrap();
        if let Some(pos) = objects
            .iter()
            .position(|existing| existing.id == object.id && existing.tenant_id == object.tenant_id)
        {
            objects[pos] = object;
            Ok(())
        } else {
            Err(RepositoryError::Database("object not found".to_string()))
        }
    }
    async fn delete_object(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        self.objects
            .lock()
            .unwrap()
            .retain(|object| !(object.id == id && object.tenant_id == tenant_id));
        Ok(())
    }

    async fn list_object_history(
        &self,
        tenant_id: Uuid,
        object_id: Uuid,
    ) -> Result<Vec<ObjectHistory>, RepositoryError> {
        let mut history = self
            .object_history
            .lock()
            .unwrap()
            .iter()
            .filter(|item| item.tenant_id == tenant_id && item.object_id == object_id)
            .cloned()
            .collect::<Vec<_>>();
        history.sort_by_key(|item| std::cmp::Reverse(item.changed_at));
        Ok(history)
    }

    async fn record_object_history(&self, history: ObjectHistory) -> Result<(), RepositoryError> {
        self.object_history.lock().unwrap().push(history);
        Ok(())
    }

    async fn get_object_type(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ObjectType>, RepositoryError> {
        let types = self.object_types.lock().unwrap();
        Ok(types.iter().find(|t| t.id == id && t.tenant_id == tenant_id).cloned())
    }

    async fn list_link_types(&self, tenant_id: Uuid) -> Result<Vec<LinkType>, RepositoryError> {
        let types = self.link_types.lock().unwrap();
        Ok(types.iter().filter(|t| t.tenant_id == tenant_id).cloned().collect())
    }
    async fn list_links(&self, tenant_id: Uuid) -> Result<Vec<Link>, RepositoryError> {
        Ok(self
            .links
            .lock()
            .unwrap()
            .iter()
            .filter(|link| link.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
    async fn create_link(&self, link: Link) -> Result<(), RepositoryError> {
        let mut links = self.links.lock().unwrap();
        if links
            .iter()
            .any(|existing| existing.id == link.id && existing.tenant_id == link.tenant_id)
        {
            return Err(RepositoryError::Database("link already exists".to_string()));
        }
        links.push(link);
        Ok(())
    }
    async fn update_link(&self, link: Link) -> Result<(), RepositoryError> {
        let mut links = self.links.lock().unwrap();
        if let Some(pos) = links
            .iter()
            .position(|existing| existing.id == link.id && existing.tenant_id == link.tenant_id)
        {
            links[pos] = link;
            Ok(())
        } else {
            Err(RepositoryError::Database("link not found".to_string()))
        }
    }
    async fn delete_link(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        self.links.lock().unwrap().retain(|link| !(link.id == id && link.tenant_id == tenant_id));
        Ok(())
    }
    async fn create_link_type(&self, link_type: LinkType) -> Result<(), RepositoryError> {
        self.link_types.lock().unwrap().push(link_type);
        Ok(())
    }
    async fn update_link_type(&self, link_type: LinkType) -> Result<(), RepositoryError> {
        let mut types = self.link_types.lock().unwrap();
        if let Some(pos) =
            types.iter().position(|t| t.id == link_type.id && t.tenant_id == link_type.tenant_id)
        {
            types[pos] = link_type;
        }
        Ok(())
    }
    async fn delete_link_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        self.link_types.lock().unwrap().retain(|t| !(t.id == id && t.tenant_id == tenant_id));
        Ok(())
    }

    async fn list_objects(
        &self,
        tenant_id: Uuid,
        object_type_id: Option<Uuid>,
    ) -> Result<Vec<Object>, RepositoryError> {
        let objects = self.objects.lock().unwrap();
        Ok(objects
            .iter()
            .filter(|o| {
                o.tenant_id == tenant_id
                    && (object_type_id.is_none() || o.object_type_id == object_type_id.unwrap())
            })
            .cloned()
            .collect())
    }

    async fn get_object(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Object>, RepositoryError> {
        let objects = self.objects.lock().unwrap();
        Ok(objects.iter().find(|o| o.id == id && o.tenant_id == tenant_id).cloned())
    }

    async fn traverse_links(
        &self,
        tenant_id: Uuid,
        source_object_id: Uuid,
        link_type_id: Uuid,
    ) -> Result<(Vec<Link>, Vec<Object>), RepositoryError> {
        let links = self.links.lock().unwrap();
        let objects = self.objects.lock().unwrap();

        let found_links: Vec<Link> = links
            .iter()
            .filter(|l| {
                l.tenant_id == tenant_id
                    && l.link_type_id == link_type_id
                    && l.source_object_id == source_object_id
            })
            .cloned()
            .collect();
        let mut targets = Vec::new();
        for link in &found_links {
            if let Some(target) =
                objects.iter().find(|o| o.id == link.target_object_id && o.tenant_id == tenant_id)
            {
                targets.push(target.clone());
            }
        }
        Ok((found_links, targets))
    }

    async fn list_action_invocations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionInvocation>, RepositoryError> {
        let invocations = self.action_invocations.lock().unwrap();
        Ok(invocations.iter().filter(|i| i.tenant_id == tenant_id).cloned().collect())
    }
    async fn list_action_types(&self, tenant_id: Uuid) -> Result<Vec<ActionType>, RepositoryError> {
        Ok(self
            .action_types
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
    async fn create_action_type(&self, action_type: ActionType) -> Result<(), RepositoryError> {
        self.action_types.lock().unwrap().push(action_type);
        Ok(())
    }
    async fn update_action_type(&self, action_type: ActionType) -> Result<(), RepositoryError> {
        let mut types = self.action_types.lock().unwrap();
        if let Some(pos) = types
            .iter()
            .position(|t| t.id == action_type.id && t.tenant_id == action_type.tenant_id)
        {
            types[pos] = action_type;
        }
        Ok(())
    }
    async fn delete_action_type(&self, tenant_id: Uuid, id: Uuid) -> Result<(), RepositoryError> {
        self.action_types.lock().unwrap().retain(|t| !(t.id == id && t.tenant_id == tenant_id));
        Ok(())
    }

    async fn list_action_type_history(
        &self,
        tenant_id: Uuid,
        action_type_id: Uuid,
    ) -> Result<Vec<ActionTypeHistory>, RepositoryError> {
        Ok(self
            .action_type_history
            .lock()
            .unwrap()
            .iter()
            .filter(|h| h.tenant_id == tenant_id && h.action_type_id == action_type_id)
            .cloned()
            .collect())
    }
    async fn record_action_type_history(
        &self,
        history: ActionTypeHistory,
    ) -> Result<(), RepositoryError> {
        self.action_type_history.lock().unwrap().push(history);
        Ok(())
    }

    async fn insert_action_invocation(
        &self,
        invocation: ActionInvocation,
    ) -> Result<(), RepositoryError> {
        self.action_invocations.lock().unwrap().push(invocation);
        Ok(())
    }

    async fn list_action_reviews(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ActionReview>, RepositoryError> {
        Ok(self
            .action_reviews
            .lock()
            .unwrap()
            .iter()
            .filter(|review| review.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn upsert_action_review(&self, review: ActionReview) -> Result<(), RepositoryError> {
        let mut reviews = self.action_reviews.lock().unwrap();
        if let Some(existing) = reviews.iter_mut().find(|item| {
            item.tenant_id == review.tenant_id && item.invocation_id == review.invocation_id
        }) {
            *existing = review;
        } else {
            reviews.push(review);
        }
        Ok(())
    }
}
