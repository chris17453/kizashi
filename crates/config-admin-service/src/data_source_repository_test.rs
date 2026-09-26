use super::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct InMemoryDataSourceRepository(pub Arc<Mutex<Vec<DataSource>>>);

#[async_trait::async_trait]
impl DataSourceRepository for InMemoryDataSourceRepository {
    async fn create(
        &self,
        value: DataSource,
        _: &str,
    ) -> Result<DataSource, DataSourceRepositoryError> {
        self.0.lock().unwrap().push(value.clone());
        Ok(value)
    }
    async fn update(
        &self,
        value: DataSource,
        _: &str,
    ) -> Result<DataSource, DataSourceRepositoryError> {
        let mut values = self.0.lock().unwrap();
        let index = values
            .iter()
            .position(|current| current.id == value.id && current.tenant_id == value.tenant_id)
            .ok_or(DataSourceRepositoryError::NotFound(value.id))?;
        values[index] = value.clone();
        Ok(value)
    }
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<DataSource>, DataSourceRepositoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|value| value.tenant_id == tenant_id && value.id == id)
            .cloned())
    }
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DataSource>, DataSourceRepositoryError> {
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
    ) -> Result<(), DataSourceRepositoryError> {
        let mut values = self.0.lock().unwrap();
        let index = values
            .iter()
            .position(|value| value.tenant_id == tenant_id && value.id == id)
            .ok_or(DataSourceRepositoryError::NotFound(id))?;
        values.remove(index);
        Ok(())
    }
}

#[test]
fn data_source_enum_values_use_stable_wire_names() {
    assert_eq!(data_source_kind_string(DataSourceKind::Cdc).unwrap(), "cdc");
    assert_eq!(data_source_mode_string(DataSourceMode::Projection).unwrap(), "projection");
}
