use super::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct InMemoryActionTemplateRepository {
    values: Arc<Mutex<Vec<ActionTemplate>>>,
}

#[async_trait::async_trait]
impl ActionTemplateRepository for InMemoryActionTemplateRepository {
    async fn create(
        &self,
        template: ActionTemplate,
        _actor: &str,
    ) -> Result<ActionTemplate, ActionTemplateRepositoryError> {
        self.values.lock().unwrap().push(template.clone());
        Ok(template)
    }
    async fn update(
        &self,
        template: ActionTemplate,
        _actor: &str,
    ) -> Result<ActionTemplate, ActionTemplateRepositoryError> {
        Ok(template)
    }
    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<ActionTemplate>, ActionTemplateRepositoryError> {
        Ok(None)
    }
    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ActionTemplate>, ActionTemplateRepositoryError> {
        Ok(Vec::new())
    }
    async fn delete(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _actor: &str,
    ) -> Result<(), ActionTemplateRepositoryError> {
        Ok(())
    }
}

#[test]
fn action_type_string_uses_wire_enum_name() {
    assert_eq!(action_type_string(ActionType::TeamsAlert).unwrap(), "teams_alert");
}

#[test]
fn invalid_action_type_rows_are_rejected() {
    let row = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        "x".to_string(),
        "".to_string(),
        "unknown".to_string(),
        serde_json::json!({}),
        1,
        chrono::Utc::now(),
        chrono::Utc::now(),
    );
    assert!(row_to_template(row).is_err());
}
