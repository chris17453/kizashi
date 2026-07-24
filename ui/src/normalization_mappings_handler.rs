#[path = "normalization_mappings_handler_test.rs"]
#[cfg(test)]
mod normalization_mappings_handler_test;

use crate::ingestion_stats_client::{RecordSearchFilter, RecordSummary};
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
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    coverage: String,
}

/// Case-insensitive substring match on source_type -- same in-handler-filter shape as the other
/// list-page searches (ADR-0062).
fn matches_query(mapping: &NormalizationMapping, q: &str) -> bool {
    q.is_empty() || mapping.source_type.to_lowercase().contains(&q.to_lowercase())
}

/// Same shape as Sensors' sortable columns (ADR-0070): applied after the search filter, on the
/// already-fetched full list (this page has no server-side pagination to preserve ordering
/// across). An unset `sort` keeps `list_mappings`' own default.
fn sort_rows(rows: &mut [NormalizationMapping], sort: &str, dir: &str) {
    match sort {
        "version" => rows.sort_by_key(|m| m.version),
        "source_type" => rows.sort_by_key(|m| m.source_type.to_lowercase()),
        _ => return,
    }
    if dir == "desc" {
        rows.reverse();
    }
}

#[derive(Template)]
#[template(path = "normalization_mappings.html")]
struct NormalizationMappingsTemplate {
    show_nav: bool,
    is_admin: bool,
    mappings: Vec<NormalizationMapping>,
    can_write: bool,
    error: Option<String>,
    form_error: Option<String>,
    q: String,
    sort: String,
    dir: String,
    coverage: Vec<MappingCoverageRow>,
    coverage_scope: String,
}

fn normalize_coverage_scope(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "unmapped" | "pending" | "complete" => value.trim().to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn matches_coverage_scope(row: &MappingCoverageRow, scope: &str) -> bool {
    match scope {
        "unmapped" => !row.mapped,
        "pending" => row.normalized_percent < 100,
        "complete" => row.total > 0 && row.normalized_percent == 100,
        _ => true,
    }
}

struct MappingCoverageRow {
    source_type: String,
    total: usize,
    normalized: usize,
    normalized_percent: i32,
    mapped: bool,
}

fn build_mapping_coverage(
    mappings: &[NormalizationMapping],
    records: &[RecordSummary],
) -> Vec<MappingCoverageRow> {
    let mapped_types = mappings
        .iter()
        .map(|mapping| mapping.source_type.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut counts = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for record in records {
        let entry = counts.entry(record.source_type.clone()).or_default();
        entry.0 += 1;
        if record.is_normalized() {
            entry.1 += 1;
        }
    }
    for mapping in mappings {
        counts.entry(mapping.source_type.clone()).or_default();
    }
    let mut rows = counts
        .into_iter()
        .map(|(source_type, (total, normalized))| MappingCoverageRow {
            mapped: mapped_types.contains(source_type.as_str()),
            source_type,
            total,
            normalized,
            normalized_percent: if total == 0 { 0 } else { (normalized * 100 / total) as i32 },
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right.total.cmp(&left.total).then_with(|| left.source_type.cmp(&right.source_type))
    });
    rows
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
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    match state.normalization_mappings_client.list_mappings(session.tenant_id).await {
        Ok(mappings) => {
            let records = state
                .stats_client
                .search_records(
                    session.tenant_id,
                    &RecordSearchFilter { limit: 1000, ..Default::default() },
                )
                .await
                .map(|result| result.records)
                .unwrap_or_default();
            let mut mappings: Vec<NormalizationMapping> =
                mappings.into_iter().filter(|m| matches_query(m, &query.q)).collect();
            sort_rows(&mut mappings, &query.sort, &query.dir);
            let mut coverage = build_mapping_coverage(&mappings, &records);
            if !query.q.is_empty() {
                coverage
                    .retain(|row| row.source_type.to_lowercase().contains(&query.q.to_lowercase()));
            }
            if query.sort == "source_type" && query.dir == "desc" {
                coverage.reverse();
            }
            let coverage_scope = normalize_coverage_scope(&query.coverage);
            if !coverage_scope.is_empty() {
                coverage.retain(|row| matches_coverage_scope(row, &coverage_scope));
                let visible = coverage
                    .iter()
                    .map(|row| row.source_type.as_str())
                    .collect::<std::collections::HashSet<_>>();
                mappings.retain(|mapping| visible.contains(mapping.source_type.as_str()));
            }
            Html(
                NormalizationMappingsTemplate {
                    show_nav: true,
                    is_admin,
                    mappings,
                    can_write,
                    error: None,
                    form_error: None,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                    coverage,
                    coverage_scope,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            NormalizationMappingsTemplate {
                show_nav: true,
                is_admin,
                mappings: vec![],
                can_write,
                error: Some(e.to_string()),
                form_error: None,
                q: query.q,
                sort: query.sort,
                dir: query.dir,
                coverage: vec![],
                coverage_scope: normalize_coverage_scope(&query.coverage),
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
    #[serde(default)]
    dedup_fields: String,
    #[serde(default)]
    dedup_window_seconds: String,
}

fn parse_dedup_fields(
    raw: &str,
    field_map: &BTreeMap<String, String>,
) -> Result<Vec<String>, &'static str> {
    let fields = raw
        .split(|c: char| c == ',' || c == '\n')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if fields.iter().any(|field| !field_map.contains_key(field)) {
        return Err("every dedup field must be one of the normalized target fields");
    }
    Ok(fields)
}

fn parse_dedup_window(raw: &str) -> Result<Option<i64>, &'static str> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let seconds = raw
        .trim()
        .parse::<i64>()
        .map_err(|_| "deduplication window must be a whole number of seconds")?;
    if seconds <= 0 {
        return Err("deduplication window must be greater than zero");
    }
    Ok(Some(seconds))
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
    let is_admin = session.role.at_least(common::Role::Admin);
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
                    is_admin,
                    mappings,
                    can_write,
                    error: None,
                    form_error: Some(msg.to_string()),
                    q: String::new(),
                    sort: String::new(),
                    dir: String::new(),
                    coverage: vec![],
                    coverage_scope: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let dedup_fields = match parse_dedup_fields(&form.dedup_fields, &field_map) {
        Ok(fields) => fields,
        Err(msg) => return render_mapping_form_error(&state, &session, msg).await,
    };
    let dedup_window_seconds = match parse_dedup_window(&form.dedup_window_seconds) {
        Ok(window) => window,
        Err(msg) => return render_mapping_form_error(&state, &session, msg).await,
    };
    let mut mapping = NormalizationMapping::new(session.tenant_id, form.source_type, field_map);
    mapping.dedup_fields = dedup_fields;
    mapping.dedup_window_seconds = dedup_window_seconds;

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
                    is_admin,
                    mappings,
                    can_write,
                    error: None,
                    form_error: Some(e.to_string()),
                    q: String::new(),
                    sort: String::new(),
                    dir: String::new(),
                    coverage: vec![],
                    coverage_scope: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    }
}

async fn render_mapping_form_error(
    state: &AppState,
    session: &crate::Session,
    message: &str,
) -> Response {
    let mappings = state
        .normalization_mappings_client
        .list_mappings(session.tenant_id)
        .await
        .unwrap_or_default();
    Html(
        NormalizationMappingsTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            mappings,
            can_write: true,
            error: None,
            form_error: Some(message.to_string()),
            q: String::new(),
            sort: String::new(),
            dir: String::new(),
            coverage: vec![],
            coverage_scope: String::new(),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct EditMappingForm {
    source_type: String,
    field_map: String,
    #[serde(default)]
    dedup_fields: String,
    #[serde(default)]
    dedup_window_seconds: String,
    version: i32,
}

pub async fn post_edit_normalization_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Form(form): Form<EditMappingForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if form.source_type.trim().is_empty() || form.version < 1 {
        return Redirect::to("/normalization-mappings").into_response();
    }
    let field_map = match parse_field_map(&form.field_map) {
        Ok(value) => value,
        Err(message) => return render_mapping_form_error(&state, &session, message).await,
    };
    let dedup_fields = match parse_dedup_fields(&form.dedup_fields, &field_map) {
        Ok(value) => value,
        Err(message) => return render_mapping_form_error(&state, &session, message).await,
    };
    let dedup_window_seconds = match parse_dedup_window(&form.dedup_window_seconds) {
        Ok(value) => value,
        Err(message) => return render_mapping_form_error(&state, &session, message).await,
    };
    let Some(existing) = state
        .normalization_mappings_client
        .list_mappings(session.tenant_id)
        .await
        .ok()
        .and_then(|items| items.into_iter().find(|mapping| mapping.id == id))
    else {
        return Redirect::to("/normalization-mappings").into_response();
    };
    let mapping = NormalizationMapping {
        id,
        tenant_id: session.tenant_id,
        source_type: form.source_type.trim().to_string(),
        field_map,
        version: form.version.max(existing.version + 1),
        dedup_fields,
        dedup_window_seconds,
    };
    match state
        .normalization_mappings_client
        .update_mapping(session.role, &session.username, mapping)
        .await
    {
        Ok(_) => Redirect::to("/normalization-mappings").into_response(),
        Err(_) => Redirect::to("/normalization-mappings").into_response(),
    }
}
