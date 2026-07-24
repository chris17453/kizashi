#[path = "branding_handler_test.rs"]
#[cfg(test)]
mod branding_handler_test;

use crate::branding_client::Branding;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "branding.html")]
struct BrandingTemplate {
    show_nav: bool,
    is_admin: bool,
    tenant_id: Uuid,
    product_name: String,
    logo_url: String,
    accent_color: String,
    can_write: bool,
    saved: bool,
    error: Option<String>,
    configured_count: usize,
}

fn configured_count(product_name: &str, logo_url: &str, accent_color: &str) -> usize {
    [product_name, logo_url, accent_color].iter().filter(|value| !value.trim().is_empty()).count()
}

fn empty_branding() -> Branding {
    Branding { product_name: None, logo_url: None, accent_color: None }
}

/// GET /branding — white-label settings (spec §1: "white-labelable"). Admin-only edit (a
/// workspace-wide identity change, not a per-user preference); anyone signed in can view the
/// current values, same visibility rule as other config pages that only gate writes.
pub async fn get_branding_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Admin);

    let (branding, error) = match state.branding_client.get_branding_by_id(session.tenant_id).await
    {
        Ok(branding) => (branding, None),
        Err(e) => (empty_branding(), Some(e.to_string())),
    };
    let product_name = branding.product_name.unwrap_or_default();
    let logo_url = branding.logo_url.unwrap_or_default();
    let accent_color = branding.accent_color.unwrap_or_default();
    let configured_count = configured_count(&product_name, &logo_url, &accent_color);

    Html(
        BrandingTemplate {
            show_nav: true,
            is_admin,
            tenant_id: session.tenant_id,
            product_name,
            logo_url,
            accent_color,
            can_write,
            saved: false,
            error,
            configured_count,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(serde::Deserialize)]
pub struct PutBrandingForm {
    #[serde(default)]
    product_name: String,
    #[serde(default)]
    logo_url: String,
    #[serde(default)]
    accent_color: String,
}

fn non_empty(s: String) -> Option<String> {
    let trimmed = s.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// POST /branding — admin-only. Blank fields save as `None` ("use the platform default"), not
/// empty strings — an operator clearing the product name field should reset to "Kizashi", not
/// render a blank brand.
pub async fn post_branding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PutBrandingForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    if !session.role.at_least(common::Role::Admin) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let branding = Branding {
        product_name: non_empty(form.product_name),
        logo_url: non_empty(form.logo_url),
        accent_color: non_empty(form.accent_color),
    };

    let (saved, error) = match state
        .branding_client
        .put_branding(session.tenant_id, session.role, &session.username, branding.clone())
        .await
    {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };
    let product_name = branding.product_name.clone().unwrap_or_default();
    let logo_url = branding.logo_url.clone().unwrap_or_default();
    let accent_color = branding.accent_color.clone().unwrap_or_default();
    let configured_count = configured_count(&product_name, &logo_url, &accent_color);

    Html(
        BrandingTemplate {
            show_nav: true,
            is_admin,
            tenant_id: session.tenant_id,
            product_name,
            logo_url,
            accent_color,
            can_write: true,
            saved,
            error,
            configured_count,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
