use super::*;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryEventsClient {
    pub events: Mutex<Vec<EventSummary>>,
    pub has_more: Mutex<bool>,
    pub daily_counts: Mutex<Vec<DailyCount>>,
}

#[async_trait]
impl EventsClient for InMemoryEventsClient {
    async fn list_events(
        &self,
        _bearer_token: &str,
        _limit: u32,
        _offset: u32,
    ) -> Result<EventsPage, EventsClientError> {
        Ok(EventsPage {
            events: self.events.lock().unwrap().clone(),
            has_more: *self.has_more.lock().unwrap(),
        })
    }

    async fn list_events_for_record(
        &self,
        _bearer_token: &str,
        _record_id: Uuid,
    ) -> Result<Vec<EventSummary>, EventsClientError> {
        Ok(self.events.lock().unwrap().clone())
    }

    async fn daily_counts(
        &self,
        _bearer_token: &str,
        _since: chrono::DateTime<chrono::Utc>,
        _until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<DailyCount>, EventsClientError> {
        Ok(self.daily_counts.lock().unwrap().clone())
    }
}

pub struct FailingEventsClient;

#[async_trait]
impl EventsClient for FailingEventsClient {
    async fn list_events(
        &self,
        _bearer_token: &str,
        _limit: u32,
        _offset: u32,
    ) -> Result<EventsPage, EventsClientError> {
        Err(EventsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn list_events_for_record(
        &self,
        _bearer_token: &str,
        _record_id: Uuid,
    ) -> Result<Vec<EventSummary>, EventsClientError> {
        Err(EventsClientError::Unreachable("simulated failure".to_string()))
    }

    async fn daily_counts(
        &self,
        _bearer_token: &str,
        _since: chrono::DateTime<chrono::Utc>,
        _until: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<DailyCount>, EventsClientError> {
        Err(EventsClientError::Unreachable("simulated failure".to_string()))
    }
}

async fn spawn_stub_server(expected_token: &'static str) -> String {
    async fn handler(headers: HeaderMap) -> axum::response::Response {
        let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
        if auth != Some("Bearer correct-token") {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Json(serde_json::json!({
            "events": [{
                "id": "11111111-1111-1111-1111-111111111111",
                "event_type": "sentiment_spike",
                "group_key": "customer-42",
                "status": "open",
                "occurred_at": "2026-07-18T00:00:00Z"
            }],
            "has_more": false
        }))
        .into_response()
    }
    async fn daily_counts_handler() -> axum::response::Response {
        Json(serde_json::json!({
            "counts": [{"date": "2026-07-18", "count": 3}]
        }))
        .into_response()
    }
    let _ = expected_token;
    let app = Router::new()
        .route("/v1/events", get(handler))
        .route("/v1/events/daily-counts", get(daily_counts_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn http_client_lists_events_against_a_real_server() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);

    let page = client.list_events("correct-token", 100, 0).await.unwrap();

    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_type, "sentiment_spike");
    assert_eq!(page.events[0].status, "open");
    assert!(!page.has_more);
}

#[tokio::test]
async fn http_client_is_rejected_with_the_wrong_token() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);

    let err = client.list_events("wrong-token", 100, 0).await.unwrap_err();
    assert!(matches!(err, EventsClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpEventsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_events("token", 100, 0).await.unwrap_err();
    assert!(matches!(err, EventsClientError::Unreachable(_)));
}

#[tokio::test]
async fn http_client_lists_events_for_a_record_against_a_real_server() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);

    let events = client.list_events_for_record("correct-token", Uuid::new_v4()).await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "sentiment_spike");
}

#[tokio::test]
async fn http_client_gets_daily_counts_against_a_real_server() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);

    let counts = client
        .daily_counts(
            "correct-token",
            chrono::Utc::now() - chrono::Duration::days(30),
            chrono::Utc::now(),
        )
        .await
        .unwrap();

    assert_eq!(counts, vec![DailyCount { date: "2026-07-18".to_string(), count: 3 }]);
}
