use async_trait::async_trait;
use common::ontology::{
    ActionInvocation, ActionReview, ActionType, ActionTypeHistory, Link, LinkType, Object,
    ObjectHistory, ObjectType,
};
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use uuid::Uuid;

static CLIENT: OnceLock<Arc<dyn OntologyClient>> = OnceLock::new();

pub fn initialize(client: Arc<dyn OntologyClient>) {
    let _ = CLIENT.set(client);
}

pub fn global() -> Option<Arc<dyn OntologyClient>> {
    CLIENT.get().cloned()
}

#[derive(Debug, Error)]
pub enum OntologyClientError {
    #[error("ontology service unreachable: {0}")]
    Unreachable(String),
    #[error("ontology service rejected the request: HTTP {0}")]
    Rejected(u16),
}

#[async_trait]
pub trait OntologyClient: Send + Sync {
    async fn list_object_types(&self, token: &str) -> Result<Vec<ObjectType>, OntologyClientError>;
    async fn list_link_types(&self, token: &str) -> Result<Vec<LinkType>, OntologyClientError>;
    async fn list_links(&self, token: &str) -> Result<Vec<Link>, OntologyClientError>;
    async fn list_objects(
        &self,
        token: &str,
        type_id: Option<Uuid>,
    ) -> Result<Vec<Object>, OntologyClientError>;
    async fn list_object_history(
        &self,
        token: &str,
        id: Uuid,
    ) -> Result<Vec<ObjectHistory>, OntologyClientError>;
    async fn create_object(
        &self,
        token: &str,
        input: &CreateObjectRequest,
    ) -> Result<(), OntologyClientError>;
    async fn update_object(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateObjectRequest,
    ) -> Result<(), OntologyClientError>;
    async fn delete_object(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError>;
    async fn list_action_invocations(
        &self,
        token: &str,
    ) -> Result<Vec<ActionInvocation>, OntologyClientError>;
    async fn list_action_reviews(
        &self,
        token: &str,
    ) -> Result<Vec<ActionReview>, OntologyClientError>;
    async fn upsert_action_review(
        &self,
        token: &str,
        input: &ActionReviewRequest,
    ) -> Result<ActionReview, OntologyClientError>;
    async fn list_action_types(&self, token: &str) -> Result<Vec<ActionType>, OntologyClientError>;
    async fn list_action_type_history(
        &self,
        token: &str,
        id: Uuid,
    ) -> Result<Vec<ActionTypeHistory>, OntologyClientError>;
    async fn invoke_action(
        &self,
        token: &str,
        input: &InvokeActionRequest,
    ) -> Result<ActionInvocation, OntologyClientError>;
    async fn traverse_links(
        &self,
        token: &str,
        object_id: Uuid,
        link_type_id: Uuid,
    ) -> Result<TraversalResult, OntologyClientError>;
    async fn create_object_type(
        &self,
        token: &str,
        input: &CreateObjectTypeRequest,
    ) -> Result<(), OntologyClientError>;
    async fn delete_object_type(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError>;
    async fn update_object_type(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateObjectTypeRequest,
    ) -> Result<(), OntologyClientError>;
    async fn create_link_type(
        &self,
        token: &str,
        input: &CreateLinkTypeRequest,
    ) -> Result<(), OntologyClientError>;
    async fn delete_link_type(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError>;
    async fn update_link_type(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateLinkTypeRequest,
    ) -> Result<(), OntologyClientError>;
    async fn create_link(
        &self,
        token: &str,
        input: &CreateLinkRequest,
    ) -> Result<(), OntologyClientError>;
    async fn update_link(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateLinkRequest,
    ) -> Result<(), OntologyClientError>;
    async fn delete_link(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError>;
    async fn create_action_type(
        &self,
        token: &str,
        input: &CreateActionTypeRequest,
    ) -> Result<(), OntologyClientError>;
    async fn delete_action_type(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError>;
    async fn update_action_type(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateActionTypeRequest,
    ) -> Result<(), OntologyClientError>;
}

#[derive(Debug, serde::Serialize)]
pub struct CreateObjectTypeRequest {
    pub name: String,
    pub version: i32,
    pub property_schema: serde_json::Value,
    pub mapping_rules: serde_json::Value,
}
#[derive(Debug, serde::Serialize)]
pub struct CreateObjectRequest {
    pub object_type_id: Uuid,
    pub properties: serde_json::Value,
    pub source_lineage: serde_json::Value,
}
#[derive(Debug, serde::Serialize)]
pub struct CreateLinkTypeRequest {
    pub name: String,
    pub source_object_type_id: Uuid,
    pub target_object_type_id: Uuid,
    pub cardinality: String,
    pub properties_schema: Option<serde_json::Value>,
}
#[derive(Debug, serde::Serialize)]
pub struct CreateLinkRequest {
    pub link_type_id: Uuid,
    pub source_object_id: Uuid,
    pub target_object_id: Uuid,
    pub properties: Option<serde_json::Value>,
}
#[derive(Debug, serde::Serialize)]
pub struct CreateActionTypeRequest {
    pub name: String,
    pub target_object_type_id: Option<Uuid>,
    pub parameter_schema: serde_json::Value,
    pub preconditions: serde_json::Value,
    pub effect_definition: serde_json::Value,
}
#[derive(Debug, serde::Serialize)]
pub struct InvokeActionRequest {
    pub action_type_id: Uuid,
    pub target_object_ids: Vec<Uuid>,
    pub parameters: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggering_event_ref: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct ActionReviewRequest {
    pub invocation_id: Uuid,
    pub status: String,
    pub assignee: Option<String>,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, serde::Deserialize)]
pub struct TraversalResult {
    pub links: Vec<Link>,
    pub targets: Vec<Object>,
}

pub struct HttpOntologyClient {
    client: reqwest::Client,
    query_gateway_url: String,
}

impl HttpOntologyClient {
    pub fn new(client: reqwest::Client, query_gateway_url: String) -> Self {
        Self { client, query_gateway_url }
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
    ) -> Result<T, OntologyClientError> {
        let response = self
            .client
            .get(format!("{}{}", self.query_gateway_url, path))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(OntologyClientError::Rejected(status.as_u16()));
        }
        response.json().await.map_err(|e| OntologyClientError::Unreachable(e.to_string()))
    }
}

#[async_trait]
impl OntologyClient for HttpOntologyClient {
    async fn list_object_types(&self, token: &str) -> Result<Vec<ObjectType>, OntologyClientError> {
        self.get(token, "/api/ontology/objects/types").await
    }

    async fn list_link_types(&self, token: &str) -> Result<Vec<LinkType>, OntologyClientError> {
        self.get(token, "/api/ontology/links/types").await
    }
    async fn list_links(&self, token: &str) -> Result<Vec<Link>, OntologyClientError> {
        self.get(token, "/api/ontology/links").await
    }

    async fn list_objects(
        &self,
        token: &str,
        type_id: Option<Uuid>,
    ) -> Result<Vec<Object>, OntologyClientError> {
        let path = match type_id {
            Some(id) => format!("/api/ontology/objects?object_type_id={id}"),
            None => "/api/ontology/objects".to_string(),
        };
        self.get(token, &path).await
    }

    async fn list_object_history(
        &self,
        token: &str,
        id: Uuid,
    ) -> Result<Vec<ObjectHistory>, OntologyClientError> {
        self.get(token, &format!("/api/ontology/objects/{id}/history")).await
    }

    async fn create_object(
        &self,
        token: &str,
        input: &CreateObjectRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, "/api/ontology/objects", input).await
    }
    async fn update_object(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateObjectRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, &format!("/api/ontology/objects/{id}"), input).await
    }
    async fn delete_object(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError> {
        self.delete(token, &format!("/api/ontology/objects/{id}")).await
    }

    async fn list_action_invocations(
        &self,
        token: &str,
    ) -> Result<Vec<ActionInvocation>, OntologyClientError> {
        self.get(token, "/api/ontology/actions/invocations").await
    }

    async fn list_action_types(&self, token: &str) -> Result<Vec<ActionType>, OntologyClientError> {
        self.get(token, "/api/ontology/actions/types").await
    }
    async fn list_action_type_history(
        &self,
        token: &str,
        id: Uuid,
    ) -> Result<Vec<ActionTypeHistory>, OntologyClientError> {
        self.get(token, &format!("/api/ontology/actions/types/{id}/history")).await
    }
    async fn list_action_reviews(
        &self,
        token: &str,
    ) -> Result<Vec<ActionReview>, OntologyClientError> {
        self.get(token, "/api/ontology/actions/reviews").await
    }
    async fn upsert_action_review(
        &self,
        token: &str,
        input: &ActionReviewRequest,
    ) -> Result<ActionReview, OntologyClientError> {
        let response = self
            .client
            .post(format!("{}/api/ontology/actions/reviews", self.query_gateway_url))
            .bearer_auth(token)
            .json(input)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(OntologyClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| OntologyClientError::Unreachable(e.to_string()))
    }
    async fn invoke_action(
        &self,
        token: &str,
        input: &InvokeActionRequest,
    ) -> Result<ActionInvocation, OntologyClientError> {
        let response = self
            .client
            .post(format!("{}/api/ontology/actions/invoke", self.query_gateway_url))
            .bearer_auth(token)
            .json(input)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        if !response.status().is_success() {
            return Err(OntologyClientError::Rejected(response.status().as_u16()));
        }
        response.json().await.map_err(|e| OntologyClientError::Unreachable(e.to_string()))
    }
    async fn traverse_links(
        &self,
        token: &str,
        object_id: Uuid,
        link_type_id: Uuid,
    ) -> Result<TraversalResult, OntologyClientError> {
        self.get(token, &format!("/api/ontology/objects/{object_id}/links/{link_type_id}")).await
    }

    async fn create_object_type(
        &self,
        token: &str,
        input: &CreateObjectTypeRequest,
    ) -> Result<(), OntologyClientError> {
        let response = self
            .client
            .post(format!("{}/api/ontology/objects/types", self.query_gateway_url))
            .bearer_auth(token)
            .json(input)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(OntologyClientError::Rejected(response.status().as_u16()))
        }
    }

    async fn delete_object_type(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError> {
        let response = self
            .client
            .delete(format!("{}/api/ontology/objects/types/{id}", self.query_gateway_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(OntologyClientError::Rejected(response.status().as_u16()))
        }
    }

    async fn update_object_type(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateObjectTypeRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, &format!("/api/ontology/objects/types/{id}"), input).await
    }

    async fn create_link_type(
        &self,
        token: &str,
        input: &CreateLinkTypeRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, "/api/ontology/links/types", input).await
    }
    async fn delete_link_type(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError> {
        self.delete(token, &format!("/api/ontology/links/types/{id}")).await
    }
    async fn update_link_type(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateLinkTypeRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, &format!("/api/ontology/links/types/{id}"), input).await
    }
    async fn create_link(
        &self,
        token: &str,
        input: &CreateLinkRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, "/api/ontology/links", input).await
    }
    async fn update_link(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateLinkRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, &format!("/api/ontology/links/{id}"), input).await
    }
    async fn delete_link(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError> {
        self.delete(token, &format!("/api/ontology/links/{id}")).await
    }
    async fn create_action_type(
        &self,
        token: &str,
        input: &CreateActionTypeRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, "/api/ontology/actions/types", input).await
    }
    async fn delete_action_type(&self, token: &str, id: Uuid) -> Result<(), OntologyClientError> {
        self.delete(token, &format!("/api/ontology/actions/types/{id}")).await
    }
    async fn update_action_type(
        &self,
        token: &str,
        id: Uuid,
        input: &CreateActionTypeRequest,
    ) -> Result<(), OntologyClientError> {
        self.post_json(token, &format!("/api/ontology/actions/types/{id}"), input).await
    }
}

impl HttpOntologyClient {
    async fn post_json<T: serde::Serialize>(
        &self,
        token: &str,
        path: &str,
        input: &T,
    ) -> Result<(), OntologyClientError> {
        let response = self
            .client
            .post(format!("{}{}", self.query_gateway_url, path))
            .bearer_auth(token)
            .json(input)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(OntologyClientError::Rejected(response.status().as_u16()))
        }
    }
    async fn delete(&self, token: &str, path: &str) -> Result<(), OntologyClientError> {
        let response = self
            .client
            .delete(format!("{}{}", self.query_gateway_url, path))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| OntologyClientError::Unreachable(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(OntologyClientError::Rejected(response.status().as_u16()))
        }
    }
}
