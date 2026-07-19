use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemorySignalRepository {
    pub signals: Mutex<Vec<AnalyzedSignal>>,
}

#[async_trait]
impl SignalRepository for InMemorySignalRepository {
    async fn record_signal(&self, signal: &AnalyzedSignal) -> Result<(), SignalRepositoryError> {
        self.signals.lock().unwrap().push(signal.clone());
        Ok(())
    }

    async fn window_stats(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        group_key: &str,
        window_seconds: i64,
    ) -> Result<(u32, Vec<f64>, Vec<Uuid>), SignalRepositoryError> {
        let cutoff = Utc::now() - chrono::Duration::seconds(window_seconds);
        let matching: Vec<AnalyzedSignal> = self
            .signals
            .lock()
            .unwrap()
            .iter()
            .filter(|s| {
                s.tenant_id == tenant_id
                    && s.event_type == event_type
                    && s.group_key == group_key
                    && s.occurred_at >= cutoff
            })
            .cloned()
            .collect();
        let count = matching.len() as u32;
        let values = matching.iter().filter_map(|s| s.numeric_value).collect();
        let record_ids = matching.iter().map(|s| s.record_id).collect();
        Ok((count, values, record_ids))
    }
}

pub struct FailingSignalRepository;

#[async_trait]
impl SignalRepository for FailingSignalRepository {
    async fn record_signal(&self, _signal: &AnalyzedSignal) -> Result<(), SignalRepositoryError> {
        Err(SignalRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn window_stats(
        &self,
        _tenant_id: Uuid,
        _event_type: &str,
        _group_key: &str,
        _window_seconds: i64,
    ) -> Result<(u32, Vec<f64>, Vec<Uuid>), SignalRepositoryError> {
        Err(SignalRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_signal(tenant_id: Uuid, group_key: &str, occurred_at: DateTime<Utc>) -> AnalyzedSignal {
    AnalyzedSignal {
        id: Uuid::new_v4(),
        tenant_id,
        record_id: Uuid::new_v4(),
        event_type: "sentiment".to_string(),
        group_key: group_key.to_string(),
        entity_ref: group_key.to_string(),
        numeric_value: Some(-0.8),
        source_connector_id: "zendesk".to_string(),
        occurred_at,
    }
}

#[tokio::test]
async fn window_stats_counts_and_collects_values_within_the_window() {
    let repo = InMemorySignalRepository::default();
    let tenant_id = Uuid::new_v4();

    repo.record_signal(&sample_signal(tenant_id, "cust-1", Utc::now())).await.unwrap();
    repo.record_signal(&sample_signal(tenant_id, "cust-1", Utc::now())).await.unwrap();
    repo.record_signal(&sample_signal(
        tenant_id,
        "cust-1",
        Utc::now() - chrono::Duration::hours(2),
    ))
    .await
    .unwrap();

    let (count, values, record_ids) =
        repo.window_stats(tenant_id, "sentiment", "cust-1", 3600).await.unwrap();
    assert_eq!(count, 2, "the signal outside the 1h window must not count");
    assert_eq!(values, vec![-0.8, -0.8]);
    assert_eq!(record_ids.len(), 2, "each in-window signal's record id must be returned");
}

#[tokio::test]
async fn window_stats_is_scoped_to_tenant_and_group_key() {
    let repo = InMemorySignalRepository::default();
    let tenant_id = Uuid::new_v4();

    repo.record_signal(&sample_signal(tenant_id, "cust-1", Utc::now())).await.unwrap();
    repo.record_signal(&sample_signal(tenant_id, "cust-2", Utc::now())).await.unwrap();
    repo.record_signal(&sample_signal(Uuid::new_v4(), "cust-1", Utc::now())).await.unwrap();

    let (count, _, _) = repo.window_stats(tenant_id, "sentiment", "cust-1", 3600).await.unwrap();
    assert_eq!(count, 1);
}
