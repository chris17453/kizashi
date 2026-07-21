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
    pub event_detail: Mutex<Option<EventDetail>>,
}

#[async_trait]
impl EventsClient for InMemoryEventsClient {
    async fn list_events(
        &self,
        _bearer_token: &str,
        _limit: u32,
        _offset: u32,
        _since: Option<chrono::DateTime<chrono::Utc>>,
        _until: Option<chrono::DateTime<chrono::Utc>>,
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

    async fn get_event(
        &self,
        _bearer_token: &str,
        _id: Uuid,
    ) -> Result<Option<EventDetail>, EventsClientError> {
        Ok(self.event_detail.lock().unwrap().clone())
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
        _since: Option<chrono::DateTime<chrono::Utc>>,
        _until: Option<chrono::DateTime<chrono::Utc>>,
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

    async fn get_event(
        &self,
        _bearer_token: &str,
        _id: Uuid,
    ) -> Result<Option<EventDetail>, EventsClientError> {
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
                "occurred_at": "2026-07-18T00:00:00Z",
                "record_ids": ["22222222-2222-2222-2222-222222222222"]
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
    async fn get_event_handler(
        axum::extract::Path(id): axum::extract::Path<Uuid>,
    ) -> axum::response::Response {
        if id == Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap() {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        }
        Json(serde_json::json!({
            "id": id,
            "event_type": "sentiment_spike",
            "source_connector_ids": ["zendesk-1"],
            "entity_ref": "cust-42",
            "group_key": "customer-42",
            "payload": {"score": -0.8},
            "occurred_at": "2026-07-18T00:00:00Z",
            "created_at": "2026-07-18T00:00:01Z",
            "status": "triggered",
            "record_ids": ["22222222-2222-2222-2222-222222222222"]
        }))
        .into_response()
    }
    let _ = expected_token;
    let app = Router::new()
        .route("/v1/events", get(handler))
        .route("/v1/events/daily-counts", get(daily_counts_handler))
        .route("/v1/events/:id", get(get_event_handler));
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

    let page = client.list_events("correct-token", 100, 0, None, None).await.unwrap();

    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_type, "sentiment_spike");
    assert_eq!(page.events[0].status, "open");
    assert_eq!(
        page.events[0].record_ids,
        vec![Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap()]
    );
    assert!(!page.has_more);
}

#[tokio::test]
async fn http_client_sends_since_and_until_as_query_params() {
    async fn handler(
        axum::extract::Query(params): axum::extract::Query<
            std::collections::HashMap<String, String>,
        >,
    ) -> axum::response::Response {
        assert_eq!(params.get("since").map(String::as_str), Some("2026-07-15T00:00:00+00:00"));
        assert_eq!(params.get("until").map(String::as_str), Some("2026-07-20T23:59:59+00:00"));
        Json(serde_json::json!({"events": [], "has_more": false})).into_response()
    }
    let app = Router::new().route("/v1/events", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = HttpEventsClient::new(reqwest::Client::new(), format!("http://{addr}"));

    let page = client
        .list_events(
            "token",
            100,
            0,
            Some("2026-07-15T00:00:00Z".parse().unwrap()),
            Some("2026-07-20T23:59:59Z".parse().unwrap()),
        )
        .await
        .unwrap();

    assert!(page.events.is_empty());
}

#[tokio::test]
async fn http_client_is_rejected_with_the_wrong_token() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);

    let err = client.list_events("wrong-token", 100, 0, None, None).await.unwrap_err();
    assert!(matches!(err, EventsClientError::Rejected(401)));
}

#[tokio::test]
async fn http_client_returns_unreachable_when_server_is_down() {
    let client = HttpEventsClient::new(reqwest::Client::new(), "http://127.0.0.1:1".to_string());
    let err = client.list_events("token", 100, 0, None, None).await.unwrap_err();
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

#[tokio::test]
async fn http_client_gets_an_events_full_detail_against_a_real_server() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);
    let id = Uuid::new_v4();

    let event = client.get_event("correct-token", id).await.unwrap().unwrap();

    assert_eq!(event.id, id);
    assert_eq!(event.event_type, "sentiment_spike");
    assert_eq!(event.entity_ref, "cust-42");
    assert_eq!(event.status, "triggered");
    assert_eq!(event.payload, serde_json::json!({"score": -0.8}));
}

#[tokio::test]
async fn http_client_returns_none_when_the_event_is_not_found() {
    let url = spawn_stub_server("correct-token").await;
    let client = HttpEventsClient::new(reqwest::Client::new(), url);
    let id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();

    let event = client.get_event("correct-token", id).await.unwrap();

    assert!(event.is_none());
}
