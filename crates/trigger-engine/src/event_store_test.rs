use super::*;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use common::Event;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryEventStore {
    pub events: Mutex<Vec<Event>>,
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn insert_event(&self, event: &Event) -> Result<(), EventStoreError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub struct FailingEventStore;

#[async_trait]
impl EventStore for FailingEventStore {
    async fn insert_event(&self, _event: &Event) -> Result<(), EventStoreError> {
        Err(EventStoreError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_event() -> Event {
    Event::new(
        Uuid::new_v4(),
        "sentiment",
        "cust-1",
        "cust-1",
        serde_json::json!({"sentiment": -0.8}),
        chrono::Utc::now(),
    )
}

#[tokio::test]
async fn in_memory_store_records_inserted_events() {
    let store = InMemoryEventStore::default();
    let event = sample_event();

    store.insert_event(&event).await.unwrap();

    assert_eq!(store.events.lock().unwrap().len(), 1);
}

async fn spawn_stub_clickhouse(status: axum::http::StatusCode) -> String {
    async fn ok_handler() -> axum::http::StatusCode {
        axum::http::StatusCode::OK
    }
    async fn error_handler() -> axum::response::Response {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "insert failed").into_response()
    }
    let app = if status.is_success() {
        Router::new().route("/", post(ok_handler))
    } else {
        Router::new().route("/", post(error_handler))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/")
}

#[tokio::test]
async fn clickhouse_store_succeeds_against_a_real_server_returning_200() {
    let url = spawn_stub_clickhouse(axum::http::StatusCode::OK).await;
    let store = ClickHouseEventStore::new(reqwest::Client::new(), url);

    store.insert_event(&sample_event()).await.unwrap();
}

#[tokio::test]
async fn clickhouse_store_returns_rejected_on_server_error() {
    let url = spawn_stub_clickhouse(axum::http::StatusCode::INTERNAL_SERVER_ERROR).await;
    let store = ClickHouseEventStore::new(reqwest::Client::new(), url);

    let err = store.insert_event(&sample_event()).await.unwrap_err();
    assert!(matches!(err, EventStoreError::Rejected(500, _)));
}

#[tokio::test]
async fn clickhouse_store_returns_unreachable_when_server_is_down() {
    let store =
        ClickHouseEventStore::new(reqwest::Client::new(), "http://127.0.0.1:1/".to_string());
    let err = store.insert_event(&sample_event()).await.unwrap_err();
    assert!(matches!(err, EventStoreError::Unreachable(_)));
}
