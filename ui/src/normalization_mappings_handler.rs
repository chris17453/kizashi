#[path = "normalization_mappings_handler_test.rs"]
#[cfg(test)]
mod normalization_mappings_handler_test;

use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::NormalizationMapping;
use std::collections::BTreeMap;

#[derive(serde::Deserialize, Default)]
pub struct NormalizationMappingsQuery {
    #[serde(default)]
    q: String,
}

/// Case-insensitive substring match on source_type -- same in-handler-filter shape as the other
/// list-page searches (ADR-0062).
fn matches_query(mapping: &NormalizationMapping, q: &str) -> bool {
    q.is_empty() || mapping.source_type.to_lowercase().contains(&q.to_lowercase())
}

#[derive(Template)]
#[template(path = "normalization_mappings.html")]
struct NormalizationMappingsTemplate {
    show_nav: bool,
    mappings: Vec<NormalizationMapping>,
    can_write: bool,
    error: Option<String>,
    form_error: Option<String>,
    q: String,
}

/// GET /normalization-mappings — the Field Mappings page (this entity previously had zero UI
/// presence at all, not even read-only, despite having had a full CRUD API since ADR-0010).
pub async fn get_normalization_mappings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NormalizationMappingsQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);

    match state.normalization_mappings_client.list_mappings(session.tenant_id).await {
        Ok(mappings) => Html(
            NormalizationMappingsTemplate {
                show_nav: true,
                mappings: mappings.into_iter().filter(|m| matches_query(m, &query.q)).collect(),
                can_write,
                error: None,
                form_error: None,
                q: query.q,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
        Err(e) => Html(
            NormalizationMappingsTemplate {
                show_nav: true,
                mappings: vec![],
                can_write,
                error: Some(e.to_string()),
                form_error: None,
                q: query.q,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct PostMappingForm {
    source_type: String,
    /// One `target_field = $.json.path` pair per line — a textarea is the no-JS-friendly way
    /// to enter a variable number of key/value pairs (ADR-0014) without a dynamic add-row UI.
    field_map: String,
}

fn parse_field_map(raw: &str) -> Result<BTreeMap<String, String>, &'static str> {
    let mut field_map = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((field, path)) = line.split_once('=') else {
            continue;
        };
        let field = field.trim();
        let path = path.trim();
        if field.is_empty() || path.is_empty() {
            continue;
        }
        field_map.insert(field.to_string(), path.to_string());
    }
    if field_map.is_empty() {
        return Err("enter at least one \"field = $.path\" line");
    }
    Ok(field_map)
}

pub async fn post_normalization_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PostMappingForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let can_write = session.role.at_least(common::Role::Operator);
    if !can_write {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let field_map = match parse_field_map(&form.field_map) {
        Ok(m) => m,
        Err(msg) => {
            let mappings = state
                .normalization_mappings_client
                .list_mappings(session.tenant_id)
                .await
                .unwrap_or_default();
            return Html(
                NormalizationMappingsTemplate {
                    show_nav: true,
                    mappings,
                    can_write,
                    error: None,
                    form_error: Some(msg.to_string()),
                    q: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let mapping = NormalizationMapping::new(session.tenant_id, form.source_type, field_map);

    match state
        .normalization_mappings_client
        .create_mapping(session.role, &session.username, mapping)
        .await
    {
        Ok(_) => Redirect::to("/normalization-mappings").into_response(),
        Err(e) => {
            let mappings = state
                .normalization_mappings_client
                .list_mappings(session.tenant_id)
                .await
                .unwrap_or_default();
            Html(
                NormalizationMappingsTemplate {
                    show_nav: true,
                    mappings,
                    can_write,
                    error: None,
                    form_error: Some(e.to_string()),
                    q: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    }
}
