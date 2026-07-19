use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryEventQueryRepository {
    pub events: Mutex<Vec<Event>>,
}

impl InMemoryEventQueryRepository {
    pub fn with_events(events: Vec<Event>) -> Self {
        Self { events: Mutex::new(events) }
    }
}

#[async_trait]
impl EventQueryRepository for InMemoryEventQueryRepository {
    async fn list_events(
        &self,
        tenant_id: Uuid,
        filter: &EventFilter,
    ) -> Result<Vec<Event>, QueryError> {
        let mut matching: Vec<Event> = self
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .filter(|e| filter.event_type.as_deref().is_none_or(|t| e.event_type == t))
            .filter(|e| filter.group_key.as_deref().is_none_or(|g| e.group_key == g))
            .filter(|e| filter.status.is_none_or(|s| e.status == s))
            .filter(|e| filter.since.is_none_or(|s| e.occurred_at >= s))
            .filter(|e| filter.until.is_none_or(|u| e.occurred_at <= u))
            .cloned()
            .collect();
        matching.sort_by_key(|e| std::cmp::Reverse(e.occurred_at));
        let limit = filter.limit.clamp(1, 1000) as usize;
        let matching = matching.into_iter().skip(filter.offset as usize).take(limit).collect();
        Ok(matching)
    }

    async fn get_event(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Event>, QueryError> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.tenant_id == tenant_id && e.id == id)
            .cloned())
    }
}

pub struct FailingEventQueryRepository;

#[async_trait]
impl EventQueryRepository for FailingEventQueryRepository {
    async fn list_events(
        &self,
        _tenant_id: Uuid,
        _filter: &EventFilter,
    ) -> Result<Vec<Event>, QueryError> {
        Err(QueryError::Unreachable("simulated failure".to_string()))
    }

    async fn get_event(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<Event>, QueryError> {
        Err(QueryError::Unreachable("simulated failure".to_string()))
    }
}

fn sample_event(tenant_id: Uuid) -> Event {
    Event::new(tenant_id, "sentiment", "cust-1", "cust-1", serde_json::json!({}), Utc::now())
}

#[tokio::test]
async fn list_events_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let other_tenant_event = sample_event(Uuid::new_v4());
    let repo = InMemoryEventQueryRepository::with_events(vec![
        sample_event(tenant_id),
        other_tenant_event,
    ]);

    let found = repo
        .list_events(tenant_id, &EventFilter { limit: 10, ..Default::default() })
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn list_events_offset_skips_earlier_pages() {
    let tenant_id = Uuid::new_v4();
    let now = Utc::now();
    let mut events = Vec::new();
    for days_ago in 0..3 {
        let mut e = sample_event(tenant_id);
        e.occurred_at = now - chrono::Duration::days(days_ago);
        events.push(e);
    }
    let repo = InMemoryEventQueryRepository::with_events(events.clone());

    let found = repo
        .list_events(tenant_id, &EventFilter { limit: 1, offset: 1, ..Default::default() })
        .await
        .unwrap();
    assert_eq!(found, vec![events[1].clone()]);
}

#[tokio::test]
async fn get_event_returns_none_for_a_different_tenant() {
    let tenant_id = Uuid::new_v4();
    let event = sample_event(tenant_id);
    let repo = InMemoryEventQueryRepository::with_events(vec![event.clone()]);

    let found = repo.get_event(Uuid::new_v4(), event.id).await.unwrap();
    assert!(found.is_none());
}

#[test]
fn parse_clickhouse_datetime_parses_the_expected_format() {
    let parsed = parse_clickhouse_datetime("2026-07-18 12:34:56.789").unwrap();
    assert_eq!(parsed.format("%Y-%m-%d %H:%M:%S%.3f").to_string(), "2026-07-18 12:34:56.789");
}

#[test]
fn parse_clickhouse_datetime_rejects_garbage() {
    assert!(parse_clickhouse_datetime("not a date").is_err());
}

async fn spawn_stub_clickhouse(body: String, status: axum::http::StatusCode) -> String {
    async fn handler(
        axum::extract::State((body, status)): axum::extract::State<(
            String,
            axum::http::StatusCode,
        )>,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        (status, body).into_response()
    }
    let app =
        axum::Router::new().route("/", axum::routing::post(handler)).with_state((body, status));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/")
}

#[tokio::test]
async fn clickhouse_repository_parses_jsoneachrow_response() {
    let tenant_id = Uuid::new_v4();
    let row = serde_json::json!({
        "id": Uuid::new_v4(), "tenant_id": tenant_id, "event_type": "sentiment",
        "source_connector_ids": ["zendesk"], "entity_ref": "cust-1", "group_key": "cust-1",
        "payload": "{\"value\":-0.8}", "occurred_at": "2026-07-18 12:00:00.000",
        "created_at": "2026-07-18 12:00:01.000", "status": "new"
    });
    let url = spawn_stub_clickhouse(row.to_string(), axum::http::StatusCode::OK).await;
    let repo = ClickHouseEventQueryRepository::new(reqwest::Client::new(), url);

    let events = repo
        .list_events(tenant_id, &EventFilter { limit: 10, ..Default::default() })
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "sentiment");
}

#[tokio::test]
async fn clickhouse_repository_returns_rejected_on_server_error() {
    let url =
        spawn_stub_clickhouse("boom".to_string(), axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            .await;
    let repo = ClickHouseEventQueryRepository::new(reqwest::Client::new(), url);

    let err = repo
        .list_events(Uuid::new_v4(), &EventFilter { limit: 10, ..Default::default() })
        .await
        .unwrap_err();
    assert!(matches!(err, QueryError::Rejected(500, _)));
}

#[tokio::test]
async fn clickhouse_repository_returns_unreachable_when_server_is_down() {
    let repo = ClickHouseEventQueryRepository::new(
        reqwest::Client::new(),
        "http://127.0.0.1:1/".to_string(),
    );
    let err = repo
        .list_events(Uuid::new_v4(), &EventFilter { limit: 10, ..Default::default() })
        .await
        .unwrap_err();
    assert!(matches!(err, QueryError::Unreachable(_)));
}

/// Real ClickHouse rejects a bodyless POST with 411 Length Required (no Transfer-Encoding,
/// no Content-Length) — the stub servers above don't enforce this, so they didn't catch it
/// when the request was first built without an explicit Content-Length. This asserts the
/// request actually carries one, matching ClickHouse's real requirement.
#[tokio::test]
async fn requests_always_carry_a_content_length_header() {
    async fn handler(headers: axum::http::HeaderMap) -> axum::response::Response {
        use axum::response::IntoResponse;
        if headers.contains_key(reqwest::header::CONTENT_LENGTH) {
            axum::http::StatusCode::OK.into_response()
        } else {
            (axum::http::StatusCode::LENGTH_REQUIRED, "no content-length").into_response()
        }
    }
    let app = axum::Router::new().route("/", axum::routing::post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let url = format!("http://{addr}/");

    let repo = ClickHouseEventQueryRepository::new(reqwest::Client::new(), url);
    let result =
        repo.list_events(Uuid::new_v4(), &EventFilter { limit: 10, ..Default::default() }).await;

    // Empty body from our 200-with-no-JSON-lines response parses to an empty Vec, not an error.
    assert!(
        result.is_ok(),
        "expected Content-Length to be present so the stub returns 200: {result:?}"
    );
}
