use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryNormalizationMappingRepository {
    pub mappings: Mutex<Vec<NormalizationMapping>>,
}

impl InMemoryNormalizationMappingRepository {
    pub fn with_mapping(mapping: NormalizationMapping) -> Self {
        Self { mappings: Mutex::new(vec![mapping]) }
    }
}

#[async_trait]
impl NormalizationMappingRepository for InMemoryNormalizationMappingRepository {
    async fn create(
        &self,
        mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError> {
        self.mappings.lock().unwrap().push(mapping.clone());
        Ok(mapping)
    }

    async fn update(
        &self,
        mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        match mappings.iter_mut().find(|m| m.id == mapping.id && m.tenant_id == mapping.tenant_id) {
            Some(existing) => {
                *existing = mapping.clone();
                Ok(mapping)
            }
            None => Err(NormalizationMappingRepositoryError::NotFound(mapping.id)),
        }
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<NormalizationMapping>, NormalizationMappingRepositoryError> {
        Ok(self
            .mappings
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.id == id && m.tenant_id == tenant_id)
            .cloned())
    }

    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingRepositoryError> {
        Ok(self
            .mappings
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
}

pub struct FailingNormalizationMappingRepository;

#[async_trait]
impl NormalizationMappingRepository for FailingNormalizationMappingRepository {
    async fn create(
        &self,
        _mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError> {
        Err(NormalizationMappingRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update(
        &self,
        _mapping: NormalizationMapping,
    ) -> Result<NormalizationMapping, NormalizationMappingRepositoryError> {
        Err(NormalizationMappingRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<NormalizationMapping>, NormalizationMappingRepositoryError> {
        Err(NormalizationMappingRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<NormalizationMapping>, NormalizationMappingRepositoryError> {
        Err(NormalizationMappingRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_mapping(tenant_id: Uuid) -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping::new(tenant_id, "ticket", field_map)
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let repo = InMemoryNormalizationMappingRepository::default();
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);

    repo.create(mapping.clone()).await.unwrap();
    let found = repo.get(tenant_id, mapping.id).await.unwrap();
    assert_eq!(found, Some(mapping));
}

#[tokio::test]
async fn update_of_unknown_mapping_returns_not_found() {
    let repo = InMemoryNormalizationMappingRepository::default();
    let mapping = sample_mapping(Uuid::new_v4());

    let err = repo.update(mapping).await.unwrap_err();
    assert!(matches!(err, NormalizationMappingRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryNormalizationMappingRepository::with_mapping(sample_mapping(tenant_id));
    repo.create(sample_mapping(Uuid::new_v4())).await.unwrap();

    let found = repo.list(tenant_id).await.unwrap();
    assert_eq!(found.len(), 1);
}
