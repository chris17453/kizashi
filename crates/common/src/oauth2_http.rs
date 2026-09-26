#[path = "oauth2_http_test.rs"]
#[cfg(test)]
mod oauth2_http_test;

use oauth2::{HttpRequest, HttpResponse};

#[derive(Debug, thiserror::Error)]
pub enum OAuth2HttpError {
    #[error("oauth2 HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("oauth2 HTTP translation error: {0}")]
    Translation(String),
}

/// Executes an `oauth2` v4 token request on a caller-supplied `reqwest` 0.12 client.
///
/// `oauth2` v4's built-in `reqwest` feature drags in `reqwest` 0.11 → hyper 0.14 → h2 0.3,
/// which has an unpatched advisory (RUSTSEC-2026-0258). Keeping `oauth2` HTTP-agnostic and
/// routing through the workspace's `reqwest` 0.12 removes that stack entirely, and lets every
/// caller pass its own (possibly Egress-Gateway-proxied, ADR-0025) client. `oauth2` v4 and
/// `reqwest` 0.12 use different major versions of the `http` crate, so method, status and
/// headers are translated at the byte level.
pub async fn execute_oauth2_request(
    client: &reqwest::Client,
    request: HttpRequest,
) -> Result<HttpResponse, OAuth2HttpError> {
    let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(|e| OAuth2HttpError::Translation(e.to_string()))?;
    let mut builder = client.request(method, request.url.as_str()).body(request.body);
    for (name, value) in &request.headers {
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    let response = client.execute(builder.build()?).await?;

    let status_code = oauth2::http::StatusCode::from_u16(response.status().as_u16())
        .map_err(|e| OAuth2HttpError::Translation(e.to_string()))?;
    let mut headers = oauth2::http::HeaderMap::new();
    for (name, value) in response.headers() {
        let name = oauth2::http::HeaderName::from_bytes(name.as_str().as_bytes())
            .map_err(|e| OAuth2HttpError::Translation(e.to_string()))?;
        let value = oauth2::http::HeaderValue::from_bytes(value.as_bytes())
            .map_err(|e| OAuth2HttpError::Translation(e.to_string()))?;
        headers.append(name, value);
    }
    let body = response.bytes().await?.to_vec();

    Ok(HttpResponse { status_code, headers, body })
}
