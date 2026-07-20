use super::*;
use crate::session::InMemorySessionStore;
use axum::http::HeaderValue;

fn headers_with_cookie(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(axum::http::header::COOKIE, HeaderValue::from_str(value).unwrap());
    headers
}

#[test]
fn extracts_the_session_cookie_among_multiple_cookies() {
    let headers = headers_with_cookie("other=1; kizashi_session=abc-123; another=2");
    assert_eq!(session_cookie_value(&headers), Some("abc-123".to_string()));
}

#[test]
fn returns_none_when_the_cookie_is_absent() {
    let headers = headers_with_cookie("other=1");
    assert_eq!(session_cookie_value(&headers), None);
}

#[test]
fn returns_none_when_there_is_no_cookie_header_at_all() {
    assert_eq!(session_cookie_value(&HeaderMap::new()), None);
}

#[tokio::test]
async fn require_session_returns_the_session_for_a_valid_cookie() {
    let store = InMemorySessionStore::default();
    let session = Session {
        bearer_token: "tok".to_string(),
        tenant_id: uuid::Uuid::new_v4(),
        username: "alice".to_string(),
        role: common::Role::Admin,
        created_at: chrono::Utc::now(),
    };
    let session_id = store.create(session.clone()).await;
    let headers = headers_with_cookie(&format!("kizashi_session={session_id}"));

    let found = require_session(&store, &headers).await.unwrap();
    assert_eq!(found, session);
}

#[tokio::test]
async fn require_session_redirects_when_the_cookie_is_missing() {
    let store = InMemorySessionStore::default();
    let response = require_session(&store, &HeaderMap::new()).await.unwrap_err();
    assert_eq!(response.status(), axum::http::StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn require_session_redirects_when_the_session_is_unknown() {
    let store = InMemorySessionStore::default();
    let headers = headers_with_cookie("kizashi_session=unknown-id");
    let response = require_session(&store, &headers).await.unwrap_err();
    assert_eq!(response.status(), axum::http::StatusCode::SEE_OTHER);
}
