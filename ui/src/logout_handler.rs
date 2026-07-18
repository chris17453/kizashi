#[path = "logout_handler_test.rs"]
#[cfg(test)]
mod logout_handler_test;

use crate::session_guard::session_cookie_value;
use crate::{AppState, SESSION_COOKIE_NAME};
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};

pub async fn get_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(session_id) = session_cookie_value(&headers) {
        state.session_store.delete(&session_id).await;
    }

    let expire_cookie = format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; Max-Age=0");
    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(SET_COOKIE, expire_cookie.parse().unwrap());
    response
}
