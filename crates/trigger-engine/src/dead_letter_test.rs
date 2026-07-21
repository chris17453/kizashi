use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryDeadLetterManager {
    pub queue: Mutex<Vec<Vec<u8>>>,
}

#[async_trait]
impl DeadLetterManager for InMemoryDeadLetterManager {
    async fn count(&self) -> Result<u32, DeadLetterError> {
        Ok(self.queue.lock().unwrap().len() as u32)
    }

    async fn replay_oldest(&self) -> Result<bool, DeadLetterError> {
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(false);
        }
        queue.remove(0);
        Ok(true)
    }
}

pub struct FailingDeadLetterManager;

#[async_trait]
impl DeadLetterManager for FailingDeadLetterManager {
    async fn count(&self) -> Result<u32, DeadLetterError> {
        Err(DeadLetterError::Backend("simulated failure".to_string()))
    }

    async fn replay_oldest(&self) -> Result<bool, DeadLetterError> {
        Err(DeadLetterError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn count_reflects_the_number_of_queued_messages() {
    let manager = InMemoryDeadLetterManager::default();
    manager.queue.lock().unwrap().push(b"one".to_vec());
    manager.queue.lock().unwrap().push(b"two".to_vec());

    assert_eq!(manager.count().await.unwrap(), 2);
}

#[tokio::test]
async fn replay_oldest_removes_one_message_and_returns_true() {
    let manager = InMemoryDeadLetterManager::default();
    manager.queue.lock().unwrap().push(b"one".to_vec());
    manager.queue.lock().unwrap().push(b"two".to_vec());

    let replayed = manager.replay_oldest().await.unwrap();

    assert!(replayed);
    assert_eq!(manager.count().await.unwrap(), 1);
}

#[tokio::test]
async fn replay_oldest_returns_false_when_the_queue_is_empty() {
    let manager = InMemoryDeadLetterManager::default();

    let replayed = manager.replay_oldest().await.unwrap();

    assert!(!replayed);
}
