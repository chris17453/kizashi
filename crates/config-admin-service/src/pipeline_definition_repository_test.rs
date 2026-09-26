use super::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct InMemoryPipelineDefinitionRepository(pub Arc<Mutex<Vec<PipelineDefinition>>>);

#[async_trait::async_trait]
impl PipelineDefinitionRepository for InMemoryPipelineDefinitionRepository {
    async fn create(
        &self,
        value: PipelineDefinition,
        _: &str,
    ) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError> {
        self.0.lock().unwrap().push(value.clone());
        Ok(value)
    }
    async fn update(
        &self,
        mut value: PipelineDefinition,
        _: &str,
    ) -> Result<PipelineDefinition, PipelineDefinitionRepositoryError> {
        let mut values = self.0.lock().unwrap();
        let current = values
            .iter()
            .position(|current| current.id == value.id && current.tenant_id == value.tenant_id)
            .ok_or(PipelineDefinitionRepositoryError::NotFound(value.id))?;
        value.version = values[current].version + 1;
        value.created_at = values[current].created_at;
        values[current] = value.clone();
        Ok(value)
    }
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PipelineDefinition>, PipelineDefinitionRepositoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|value| value.id == id && value.tenant_id == tenant_id)
            .cloned())
    }
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<PipelineDefinition>, PipelineDefinitionRepositoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|value| value.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
    async fn delete(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        _: &str,
    ) -> Result<(), PipelineDefinitionRepositoryError> {
        let mut values = self.0.lock().unwrap();
        let current = values
            .iter()
            .position(|value| value.id == id && value.tenant_id == tenant_id)
            .ok_or(PipelineDefinitionRepositoryError::NotFound(id))?;
        values.remove(current);
        Ok(())
    }
}

#[test]
fn pipeline_mode_uses_stable_wire_names() {
    assert_eq!(mode(PipelineMode::Projection).unwrap(), "projection");
}
