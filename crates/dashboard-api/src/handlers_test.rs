use super::*;
use crate::event_query_repository::event_query_repository_test::{
    FailingEventQueryRepository, InMemoryEventQueryRepository,
};
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use common::Event;
use tower::ServiceExt;

fn router(state: DashboardState) -> Router {
    Router::new()
        .route("/v1/events", get(list_events))
        .route("/v1/events/daily-counts", get(daily_event_counts))
        .route("/v1/events/:id", axum::routing::get(get_event).patch(update_event_status))
        .with_state(state)
}

fn sample_event(tenant_id: Uuid) -> Event {
    Event::new(tenant_id, "sentiment", "cust-1", "cust-1", serde_json::json!({}), Utc::now())
}

#[tokio::test]
async fn update_event_status_is_tenant_scoped_and_returns_new_state() {
    let tenant_id = Uuid::new_v4();
    let event = sample_event(tenant_id);
    let event_id = event.id;
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::with_events(vec![event])),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/events/{event_id}"))
                .header("x-tenant-id", tenant_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"dismissed"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], "dismissed");
}

#[tokio::test]
async fn list_events_requires_tenant_header() {
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::default()),
    };

    let response = router(state)
        .oneshot(Request::builder().uri("/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_events_returns_events_scoped_to_the_tenant_header() {
    let tenant_id = Uuid::new_v4();
    let event = sample_event(tenant_id);
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::with_events(vec![
            event.clone()
        ])),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let events: Vec<Event> = serde_json::from_value(body["events"].clone()).unwrap();
    assert_eq!(events, vec![event]);
    assert_eq!(body["has_more"], serde_json::json!(false));
}

#[tokio::test]
async fn list_events_reports_has_more_when_results_exceed_the_page_size() {
    let tenant_id = Uuid::new_v4();
    let events: Vec<Event> = (0..3).map(|_| sample_event(tenant_id)).collect();
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::with_events(events)),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=2")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["events"].as_array().unwrap().len(), 2);
    assert_eq!(body["has_more"], serde_json::json!(true));
}

#[tokio::test]
async fn list_events_applies_case_insensitive_search_before_pagination() {
    let tenant_id = Uuid::new_v4();
    let mut matching = sample_event(tenant_id);
    matching.event_type = "urgent_ticket".to_string();
    let mut other = sample_event(tenant_id);
    other.event_type = "routine_ticket".to_string();
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::with_events(vec![
            other,
            matching.clone(),
        ])),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events?search=URGENT&limit=1")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let events: Vec<Event> = serde_json::from_value(body["events"].clone()).unwrap();
    assert_eq!(events, vec![matching]);
    assert_eq!(body["has_more"], serde_json::json!(false));
}

#[tokio::test]
async fn list_events_rejects_an_invalid_status_filter() {
    let tenant_id = Uuid::new_v4();
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::default()),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events?status=not-a-status")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_events_returns_500_on_repository_failure() {
    let tenant_id = Uuid::new_v4();
    let state = DashboardState { event_query_repository: Arc::new(FailingEventQueryRepository) };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}

#[tokio::test]
async fn get_event_returns_404_for_unknown_id() {
    let tenant_id = Uuid::new_v4();
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::default()),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/{}", Uuid::new_v4()))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_event_returns_500_on_repository_failure() {
    let tenant_id = Uuid::new_v4();
    let state = DashboardState { event_query_repository: Arc::new(FailingEventQueryRepository) };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/{}", Uuid::new_v4()))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}

#[tokio::test]
async fn get_event_returns_200_for_a_known_event_in_the_caller_tenant() {
    let tenant_id = Uuid::new_v4();
    let event = sample_event(tenant_id);
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::with_events(vec![
            event.clone()
        ])),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/{}", event.id))
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_event_with_invalid_tenant_header_returns_400() {
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::default()),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/{}", Uuid::new_v4()))
                .header("x-tenant-id", "not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn daily_event_counts_requires_tenant_header() {
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::default()),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/events/daily-counts?since=2026-07-01T00:00:00Z&until=2026-07-20T00:00:00Z",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn daily_event_counts_returns_buckets_for_the_caller_tenant() {
    let tenant_id = Uuid::new_v4();
    let mut event = sample_event(tenant_id);
    event.occurred_at = DateTime::parse_from_rfc3339("2026-07-15T10:00:00Z").unwrap().to_utc();
    let state = DashboardState {
        event_query_repository: Arc::new(InMemoryEventQueryRepository::with_events(vec![event])),
    };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/events/daily-counts?since=2026-07-01T00:00:00Z&until=2026-07-20T00:00:00Z",
                )
                .header("x-tenant-id", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["counts"][0]["date"], "2026-07-15");
    assert_eq!(body["counts"][0]["count"], 1);
}

#[tokio::test]
async fn daily_event_counts_returns_500_on_repository_failure() {
    let state = DashboardState { event_query_repository: Arc::new(FailingEventQueryRepository) };

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/events/daily-counts?since=2026-07-01T00:00:00Z&until=2026-07-20T00:00:00Z",
                )
                .header("x-tenant-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("simulated failure"));
}
