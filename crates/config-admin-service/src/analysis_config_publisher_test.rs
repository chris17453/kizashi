use super::*;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryAnalysisConfigPublisher {
    pub published: Mutex<Vec<AnalysisConfig>>,
}

#[async_trait]
impl AnalysisConfigPublisher for InMemoryAnalysisConfigPublisher {
    async fn publish_analysis_config_changed(
        &self,
        config: &AnalysisConfig,
    ) -> Result<(), AnalysisConfigPublishError> {
        self.published.lock().unwrap().push(config.clone());
        Ok(())
    }
}

pub struct FailingAnalysisConfigPublisher;

#[async_trait]
impl AnalysisConfigPublisher for FailingAnalysisConfigPublisher {
    async fn publish_analysis_config_changed(
        &self,
        _config: &AnalysisConfig,
    ) -> Result<(), AnalysisConfigPublishError> {
        Err(AnalysisConfigPublishError::Bus("simulated bus failure".to_string()))
    }
}

fn sample_config() -> AnalysisConfig {
    AnalysisConfig::new(Uuid::new_v4(), "look for urgent tickets")
}

#[tokio::test]
async fn in_memory_publisher_records_published_configs() {
    let publisher = InMemoryAnalysisConfigPublisher::default();
    let config = sample_config();

    publisher.publish_analysis_config_changed(&config).await.unwrap();

    let published = publisher.published.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0], config);
}

#[tokio::test]
async fn failing_publisher_returns_bus_error() {
    let publisher = FailingAnalysisConfigPublisher;
    let err = publisher.publish_analysis_config_changed(&sample_config()).await.unwrap_err();
    assert!(matches!(err, AnalysisConfigPublishError::Bus(_)));
}
