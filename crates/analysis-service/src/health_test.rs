use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_200_when_the_consumer_has_ticked_recently() {
    let heartbeat = Arc::new(ConsumerHeartbeat::new());
    heartbeat.tick();
    let app = build_router(heartbeat);
    let response =
        app.oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn healthz_returns_503_when_the_consumer_has_not_ticked_within_the_staleness_window() {
    let heartbeat = Arc::new(ConsumerHeartbeat::new());
    {
        let mut last_tick = heartbeat.last_tick.lock().unwrap();
        *last_tick = Instant::now() - (STALE_THRESHOLD + Duration::from_secs(1));
    }
    let app = build_router(heartbeat);
    let response =
        app.oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn tick_resets_the_heartbeat_to_alive() {
    let heartbeat = ConsumerHeartbeat::new();
    {
        let mut last_tick = heartbeat.last_tick.lock().unwrap();
        *last_tick = Instant::now() - (STALE_THRESHOLD + Duration::from_secs(1));
    }
    assert!(!heartbeat.is_alive());
    heartbeat.tick();
    assert!(heartbeat.is_alive());
}
