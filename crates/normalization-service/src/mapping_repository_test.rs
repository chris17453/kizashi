use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryMappingRepository {
    pub mappings: Mutex<Vec<NormalizationMapping>>,
}

impl InMemoryMappingRepository {
    pub fn with_mapping(mapping: NormalizationMapping) -> Self {
        Self { mappings: Mutex::new(vec![mapping]) }
    }
}

#[async_trait]
impl MappingRepository for InMemoryMappingRepository {
    async fn active_mapping(
        &self,
        tenant_id: Uuid,
        source_type: &str,
    ) -> Result<Option<NormalizationMapping>, MappingRepositoryError> {
        Ok(self
            .mappings
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.tenant_id == tenant_id && m.source_type == source_type)
            .max_by_key(|m| m.version)
            .cloned())
    }

    async fn upsert(&self, mapping: NormalizationMapping) -> Result<(), MappingRepositoryError> {
        let mut mappings = self.mappings.lock().unwrap();
        match mappings.iter_mut().find(|m| m.id == mapping.id) {
            Some(existing) => *existing = mapping,
            None => mappings.push(mapping),
        }
        Ok(())
    }
}

fn sample_mapping(tenant_id: Uuid) -> NormalizationMapping {
    let mut field_map = BTreeMap::new();
    field_map.insert("text".to_string(), "$.description".to_string());
    NormalizationMapping::new(tenant_id, "ticket", field_map)
}

#[tokio::test]
async fn returns_the_matching_mapping_for_tenant_and_source_type() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryMappingRepository::with_mapping(sample_mapping(tenant_id));

    let found = repo.active_mapping(tenant_id, "ticket").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().tenant_id, tenant_id);
}

#[tokio::test]
async fn returns_none_when_no_mapping_matches() {
    let repo = InMemoryMappingRepository::default();
    let found = repo.active_mapping(Uuid::new_v4(), "ticket").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn returns_highest_version_when_multiple_exist() {
    let tenant_id = Uuid::new_v4();
    let v1 = sample_mapping(tenant_id);
    let v2 = NormalizationMapping { version: 2, ..sample_mapping(tenant_id) };
    let repo = InMemoryMappingRepository { mappings: Mutex::new(vec![v1, v2.clone()]) };

    let found = repo.active_mapping(tenant_id, "ticket").await.unwrap().unwrap();
    assert_eq!(found.version, v2.version);
}

#[tokio::test]
async fn upsert_inserts_a_new_mapping() {
    let repo = InMemoryMappingRepository::default();
    let mapping = sample_mapping(Uuid::new_v4());

    repo.upsert(mapping.clone()).await.unwrap();

    let found = repo.active_mapping(mapping.tenant_id, &mapping.source_type).await.unwrap();
    assert_eq!(found, Some(mapping));
}

#[tokio::test]
async fn upsert_replaces_an_existing_mapping_with_the_same_id() {
    let tenant_id = Uuid::new_v4();
    let mapping = sample_mapping(tenant_id);
    let repo = InMemoryMappingRepository::with_mapping(mapping.clone());

    let mut updated = mapping.clone();
    updated.field_map.insert("urgency".to_string(), "$.priority".to_string());
    repo.upsert(updated.clone()).await.unwrap();

    let found = repo.active_mapping(tenant_id, "ticket").await.unwrap();
    assert_eq!(found, Some(updated));
}
