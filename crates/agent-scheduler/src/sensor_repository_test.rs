use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySensorRepository {
    pub sensors: Mutex<Vec<StoredSensor>>,
}

#[async_trait]
impl SensorRepository for InMemorySensorRepository {
    async fn upsert(&self, sensor: Sensor) -> Result<(), SensorRepositoryError> {
        let mut sensors = self.sensors.lock().unwrap();
        match sensors.iter_mut().find(|a| a.sensor.id == sensor.id) {
            Some(existing) => existing.sensor = sensor,
            None => {
                sensors.push(StoredSensor { sensor, last_polled_at: None, last_checkpoint: None })
            }
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), SensorRepositoryError> {
        self.sensors.lock().unwrap().retain(|a| a.sensor.id != id);
        Ok(())
    }

    async fn list_enabled(&self) -> Result<Vec<StoredSensor>, SensorRepositoryError> {
        Ok(self.sensors.lock().unwrap().iter().filter(|a| a.sensor.enabled).cloned().collect())
    }

    async fn mark_polled(
        &self,
        id: Uuid,
        at: chrono::DateTime<chrono::Utc>,
        checkpoint: Option<String>,
    ) -> Result<(), SensorRepositoryError> {
        let mut sensors = self.sensors.lock().unwrap();
        if let Some(found) = sensors.iter_mut().find(|a| a.sensor.id == id) {
            found.last_polled_at = Some(at);
            if checkpoint.is_some() {
                found.last_checkpoint = checkpoint;
            }
        }
        Ok(())
    }
}

fn sample_sensor(enabled: bool) -> Sensor {
    Sensor { enabled, ..Sensor::new(Uuid::new_v4(), "zendesk", "poller", serde_json::json!({})) }
}

#[tokio::test]
async fn upsert_inserts_a_new_sensor() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(true);

    repo.upsert(sensor.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].sensor, sensor);
    assert!(enabled[0].last_polled_at.is_none());
}

#[tokio::test]
async fn upsert_replaces_an_existing_sensor_by_id() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(true);
    repo.upsert(sensor.clone()).await.unwrap();

    let mut renamed = sensor.clone();
    renamed.name = "renamed".to_string();
    repo.upsert(renamed.clone()).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].sensor.name, "renamed");
}

#[tokio::test]
async fn list_enabled_excludes_disabled_sensors() {
    let repo = InMemorySensorRepository::default();
    repo.upsert(sample_sensor(true)).await.unwrap();
    repo.upsert(sample_sensor(false)).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
}

#[tokio::test]
async fn delete_removes_the_sensor() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(true);
    repo.upsert(sensor.clone()).await.unwrap();

    repo.delete(sensor.id).await.unwrap();

    assert!(repo.list_enabled().await.unwrap().is_empty());
}

#[tokio::test]
async fn mark_polled_records_the_timestamp() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(true);
    repo.upsert(sensor.clone()).await.unwrap();

    let now = chrono::Utc::now();
    repo.mark_polled(sensor.id, now, None).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled[0].last_polled_at, Some(now));
}

#[tokio::test]
async fn mark_polled_with_a_checkpoint_records_it() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(true);
    repo.upsert(sensor.clone()).await.unwrap();

    repo.mark_polled(sensor.id, chrono::Utc::now(), Some("42".to_string())).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled[0].last_checkpoint, Some("42".to_string()));
}

#[tokio::test]
async fn mark_polled_with_no_checkpoint_leaves_the_previous_one_intact() {
    let repo = InMemorySensorRepository::default();
    let sensor = sample_sensor(true);
    repo.upsert(sensor.clone()).await.unwrap();
    repo.mark_polled(sensor.id, chrono::Utc::now(), Some("42".to_string())).await.unwrap();

    repo.mark_polled(sensor.id, chrono::Utc::now(), None).await.unwrap();

    let enabled = repo.list_enabled().await.unwrap();
    assert_eq!(enabled[0].last_checkpoint, Some("42".to_string()));
}
