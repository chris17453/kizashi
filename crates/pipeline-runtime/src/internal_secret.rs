use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub async fn require_internal_secret(
    State(secret): State<String>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    if headers.get("x-internal-secret").and_then(|value| value.to_str().ok())
        != Some(secret.as_str())
    {
        return (StatusCode::UNAUTHORIZED, "invalid internal secret").into_response();
    }
    next.run(request).await
}
