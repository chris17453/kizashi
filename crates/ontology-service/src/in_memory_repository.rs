use crate::repository::{OntologyRepository, RepositoryError};
use async_trait::async_trait;
use common::ontology::{ActionInvocation, Link, LinkType, Object, ObjectType};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default, Clone)]
pub struct InMemoryOntologyRepository {
    pub object_types: Arc<Mutex<Vec<ObjectType>>>,
    pub objects: Arc<Mutex<Vec<Object>>>,
    pub link_types: Arc<Mutex<Vec<LinkType>>>,
    pub links: Arc<Mutex<Vec<Link>>>,
    pub action_invocations: Arc<Mutex<Vec<ActionInvocation>>>,
}

impl InMemoryOntologyRepository {
    pub fn new() -> Self {
        Self {
            object_types: Arc::new(Mutex::new(Vec::new())),
            objects: Arc::new(Mutex::new(Vec::new())),
            link_types: Arc::new(Mutex::new(Vec::new())),
            links: Arc::new(Mutex::new(Vec::new())),
            action_invocations: Arc::new(Mutex::new(Vec::new())),
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

    async fn upsert_object(&self, object: Object) -> Result<(), RepositoryError> {
        let mut objects = self.objects.lock().unwrap();
        if let Some(pos) = objects.iter().position(|o| o.id == object.id) {
            objects[pos] = object;
        } else {
            objects.push(object);
        }
        Ok(())
    }

    async fn get_object_type(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<ObjectType>, RepositoryError> {
        let types = self.object_types.lock().unwrap();
        Ok(types.iter().find(|t| t.id == id && t.tenant_id == tenant_id).cloned())
    }

    async fn list_link_types(&self, tenant_id: Uuid) -> Result<Vec<LinkType>, RepositoryError> {
        let types = self.link_types.lock().unwrap();
        Ok(types.iter().filter(|t| t.tenant_id == tenant_id).cloned().collect())
    }

    async fn list_objects(&self, tenant_id: Uuid, object_type_id: Option<Uuid>) -> Result<Vec<Object>, RepositoryError> {
        let objects = self.objects.lock().unwrap();
        Ok(objects.iter().filter(|o| o.tenant_id == tenant_id && (object_type_id.is_none() || o.object_type_id == object_type_id.unwrap())).cloned().collect())
    }

    async fn get_object(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Object>, RepositoryError> {
        let objects = self.objects.lock().unwrap();
        Ok(objects.iter().find(|o| o.id == id && o.tenant_id == tenant_id).cloned())
    }

    async fn traverse_links(&self, tenant_id: Uuid, source_object_id: Uuid, link_type_id: Uuid) -> Result<(Vec<Link>, Vec<Object>), RepositoryError> {
        let links = self.links.lock().unwrap();
        let objects = self.objects.lock().unwrap();
        
        let found_links: Vec<Link> = links.iter().filter(|l| l.tenant_id == tenant_id && l.link_type_id == link_type_id && l.source_object_id == source_object_id).cloned().collect();
        let mut targets = Vec::new();
        for link in &found_links {
            if let Some(target) = objects.iter().find(|o| o.id == link.target_object_id && o.tenant_id == tenant_id) {
                targets.push(target.clone());
            }
        }
        Ok((found_links, targets))
    }

    async fn list_action_invocations(&self, tenant_id: Uuid) -> Result<Vec<ActionInvocation>, RepositoryError> {
        let invocations = self.action_invocations.lock().unwrap();
        Ok(invocations.iter().filter(|i| i.tenant_id == tenant_id).cloned().collect())
    }

    async fn insert_action_invocation(&self, invocation: ActionInvocation) -> Result<(), RepositoryError> {
        self.action_invocations.lock().unwrap().push(invocation);
        Ok(())
    }
}
