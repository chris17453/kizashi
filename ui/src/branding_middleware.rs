//! Applies a tenant's white-label branding (ADR-0041) to every already-authenticated page, not
//! just the login screen — the gap ADR-0041 explicitly deferred as "a much larger, separate
//! mechanical change" (threading a branding fetch through every page handler's Askama template
//! struct individually). Implemented as ONE router-wide middleware instead: it lets the page
//! render normally, then rewrites the two fixed, known strings `layout.html` always emits for
//! branding (the nav header's product name span, the `--accent` CSS variable) rather than adding
//! a `branding` field to ~30 unrelated template structs. Brittle to a `layout.html` rewrite that
//! changes that markup, but the trade-off is deliberate: real coverage today at a fraction of the
//! per-handler-plumbing cost, not a "some day" TODO.

#[path = "branding_middleware_test.rs"]
#[cfg(test)]
mod branding_middleware_test;

use crate::branding_client::Branding;
use crate::session_guard::session_cookie_value;
use crate::AppState;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

const DEFAULT_BRAND_SPAN: &str = r#"<span class="brand-name">Kizashi</span>"#;
const DEFAULT_BRAND_MARK: &str = r#"<span class="brand-mark">&#9670;</span>"#;
const DEFAULT_DOCUMENT_TITLE: &str = "<title>Kizashi Console</title>";
const DARK_ACCENT_VAR_PREFIX: &str = "--accent: #5eead4;";
const LIGHT_ACCENT_VAR_PREFIX: &str = "--accent: #087f78;";

fn escape_html_attribute(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

fn safe_logo_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        .then(|| escape_html_attribute(trimmed))
}

fn apply_branding_to_html(html: &str, branding: &Branding) -> String {
    let mut out = html.to_string();
    if let Some(name) = &branding.product_name {
        let escaped = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        out = out
            .replace(DEFAULT_BRAND_SPAN, &format!(r#"<span class="brand-name">{escaped}</span>"#));
        out = out.replace(DEFAULT_DOCUMENT_TITLE, &format!("<title>{escaped} Console</title>"));
    }
    if let Some(logo_url) = branding.logo_url.as_deref().and_then(safe_logo_url) {
        out = out.replace(
            DEFAULT_BRAND_MARK,
            &format!(r#"<img class="brand-mark brand-logo" src="{logo_url}" alt="">"#),
        );
    }
    if let Some(color) = &branding.accent_color {
        // `accent_color` is already validated as a strict hex color server-side (ADR-0041)
        // before it's ever stored, so no further escaping is needed injecting it into CSS here.
        let replacement = format!("--accent: {color};");
        out = out
            .replace(DARK_ACCENT_VAR_PREFIX, &replacement)
            .replace(LIGHT_ACCENT_VAR_PREFIX, &replacement);
    }
    out
}

fn is_html_response(response: &Response) -> bool {
    response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"))
}

async fn tenant_id_from_session(state: &AppState, headers: &HeaderMap) -> Option<uuid::Uuid> {
    let session_id = session_cookie_value(headers)?;
    let session = state.session_store.get(&session_id).await?;
    Some(session.tenant_id)
}

pub async fn apply_branding(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers().clone();
    let response = next.run(request).await;

    if response.status() != axum::http::StatusCode::OK || !is_html_response(&response) {
        return response;
    }

    let Some(tenant_id) = tenant_id_from_session(&state, &headers).await else {
        return response;
    };
    let Ok(branding) = state.branding_client.get_branding_by_id(tenant_id).await else {
        return response;
    };
    if branding.product_name.is_none()
        && branding.logo_url.is_none()
        && branding.accent_color.is_none()
    {
        return response;
    }

    let (parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(html) = String::from_utf8(bytes.to_vec()) else {
        return Response::from_parts(parts, Body::from(bytes));
    };

    let rewritten = apply_branding_to_html(&html, &branding);
    let mut parts = parts;
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(rewritten))
}
