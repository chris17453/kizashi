use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySensorRepository {
    pub sensors: Mutex<Vec<Sensor>>,
}

impl InMemorySensorRepository {
    pub fn with_sensor(sensor: Sensor) -> Self {
        Self { sensors: Mutex::new(vec![sensor]) }
    }
}

#[async_trait]
impl SensorRepository for InMemorySensorRepository {
    async fn create(&self, sensor: Sensor) -> Result<Sensor, SensorRepositoryError> {
        self.sensors.lock().unwrap().push(sensor.clone());
        Ok(sensor)
    }

    async fn update(&self, sensor: Sensor) -> Result<Sensor, SensorRepositoryError> {
        let mut sensors = self.sensors.lock().unwrap();
        match sensors.iter_mut().find(|a| a.id == sensor.id && a.tenant_id == sensor.tenant_id) {
            Some(existing) => {
                *existing = sensor.clone();
                Ok(sensor)
            }
            None => Err(SensorRepositoryError::NotFound(sensor.id)),
        }
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sensor>, SensorRepositoryError> {
        Ok(self
            .sensors
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.id == id && a.tenant_id == tenant_id)
            .cloned())
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Sensor>, SensorRepositoryError> {
        let mut sensors: Vec<Sensor> = self
            .sensors
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.tenant_id == tenant_id)
            .cloned()
            .collect();
        sensors.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(sensors.into_iter().skip(offset as usize).take(limit as usize).collect())
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), SensorRepositoryError> {
        let mut sensors = self.sensors.lock().unwrap();
        let before_len = sensors.len();
        sensors.retain(|a| !(a.id == id && a.tenant_id == tenant_id));
        if sensors.len() == before_len {
            return Err(SensorRepositoryError::NotFound(id));
        }
        Ok(())
    }

    async fn find_by_name(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<Sensor>, SensorRepositoryError> {
        Ok(self
            .sensors
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.tenant_id == tenant_id && a.name == name)
            .cloned())
    }
}

pub struct FailingSensorRepository;

#[async_trait]
impl SensorRepository for FailingSensorRepository {
    async fn create(&self, _sensor: Sensor) -> Result<Sensor, SensorRepositoryError> {
        Err(SensorRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update(&self, _sensor: Sensor) -> Result<Sensor, SensorRepositoryError> {
        Err(SensorRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Sensor>, SensorRepositoryError> {
        Err(SensorRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Sensor>, SensorRepositoryError> {
        Err(SensorRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), SensorRepositoryError> {
        Err(SensorRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn find_by_name(
        &self,
        _tenant_id: Uuid,
        _name: &str,
    ) -> Result<Option<Sensor>, SensorRepositoryError> {
        Err(SensorRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_sensor(tenant_id: Uuid) -> Sensor {
    Sensor::new(
        tenant_id,
        "zendesk",
        "support-poller",
        serde_json::json!({"url": "https://example.zendesk.com"}),
    )
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let repo = InMemorySensorRepository::default();
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);

    repo.create(sensor.clone()).await.unwrap();
    let found = repo.get(tenant_id, sensor.id).await.unwrap();
    assert_eq!(found, Some(sensor));
}

#[tokio::test]
async fn update_of_unknown_sensor_returns_not_found() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(Uuid::new_v4());

    let err = repo.update(sensor).await.unwrap_err();
    assert!(matches!(err, SensorRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySensorRepository::with_sensor(sample_sensor(tenant_id));
    repo.create(sample_sensor(Uuid::new_v4())).await.unwrap();

    let found = repo.list(tenant_id, 25, 0).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn list_respects_limit_and_offset() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySensorRepository::default();
    for name in ["a", "b", "c"] {
        repo.create(Sensor::new(tenant_id, "zendesk", name, serde_json::json!({}))).await.unwrap();
    }

    let found = repo.list(tenant_id, 1, 1).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "b");
}

#[tokio::test]
async fn delete_removes_the_sensor() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let repo = InMemorySensorRepository::with_sensor(sensor.clone());

    repo.delete(tenant_id, sensor.id).await.unwrap();
    let found = repo.get(tenant_id, sensor.id).await.unwrap();
    assert_eq!(found, None);
}

#[tokio::test]
async fn delete_of_unknown_sensor_returns_not_found() {
    let repo = InMemorySensorRepository::default();

    let err = repo.delete(Uuid::new_v4(), Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, SensorRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn find_by_name_returns_the_matching_sensor() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let repo = InMemorySensorRepository::with_sensor(sensor.clone());

    let found = repo.find_by_name(tenant_id, "support-poller").await.unwrap();
    assert_eq!(found, Some(sensor));
}

#[tokio::test]
async fn find_by_name_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let sensor = sample_sensor(tenant_id);
    let repo = InMemorySensorRepository::with_sensor(sensor);

    let found = repo.find_by_name(Uuid::new_v4(), "support-poller").await.unwrap();
    assert_eq!(found, None);
}

#[tokio::test]
async fn find_by_name_returns_none_for_unknown_name() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemorySensorRepository::with_sensor(sample_sensor(tenant_id));

    let found = repo.find_by_name(tenant_id, "nonexistent").await.unwrap();
    assert_eq!(found, None);
}
