#[path = "triggers_handler_test.rs"]
#[cfg(test)]
mod triggers_handler_test;

use crate::ingestion_stats_client::DEFAULT_PAGE_SIZE;
use crate::session_guard::require_session;
use crate::{AppState, TriggerSummary};
use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::{ThresholdDirection, TriggerCondition, TriggerDefinition};
use uuid::Uuid;

fn default_page() -> i64 {
    0
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct TriggersQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    // ADR-0030: set together after a "Test" form submission (GET, not POST — a dry run has no
    // side effects, so it's shareable/bookmarkable/back-button-safe like any other read).
    #[serde(default)]
    pub test_trigger_id: Option<Uuid>,
    #[serde(default)]
    pub test_group_key: Option<String>,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub dir: String,
}

/// Case-insensitive substring match on name -- same shape as Users/API Keys search (ADR-0062),
/// but unlike those pages, `list_triggers` is server-paginated (ADR-0025's `limit`/`offset`),
/// so this only filters the *current page's* already-fetched triggers, not the tenant's full
/// set -- the same accepted "doesn't compose with pagination in one request" caveat ADR-0063
/// documented for Login Attempts, not a full server-side search.
fn matches_query(trigger: &TriggerSummary, q: &str) -> bool {
    q.is_empty() || trigger.name.to_lowercase().contains(&q.to_lowercase())
}

/// Same shape as Users' sortable columns (ADR-0064), applied after the search filter. Like
/// search above, this only reorders the *current page's* rows -- `list_triggers` already
/// returns `ORDER BY name` server-side, so an unset `sort` keeps that existing default rather
/// than introducing a new one.
fn sort_rows(rows: &mut [TriggerSummary], sort: &str, dir: &str) {
    match sort {
        "event_type_match" => rows.sort_by_key(|t| t.event_type_match.to_lowercase()),
        "enabled" => rows.sort_by_key(|t| !t.enabled),
        _ => rows.sort_by_key(|t| t.name.to_lowercase()),
    }
    if dir == "desc" {
        rows.reverse();
    }
}

struct TestResultView {
    group_key: String,
    would_fire: bool,
    contributing_record_count: usize,
}

#[derive(Template)]
#[template(path = "triggers.html")]
struct TriggersTemplate {
    show_nav: bool,
    is_admin: bool,
    triggers: Vec<TriggerSummary>,
    page: i64,
    has_more: bool,
    can_write: bool,
    error: Option<String>,
    form_error: Option<String>,
    test_result: Option<TestResultView>,
    q: String,
    sort: String,
    dir: String,
}

pub async fn get_triggers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TriggersQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);

    let test_result = match (query.test_trigger_id, &query.test_group_key) {
        (Some(trigger_id), Some(group_key)) if !group_key.is_empty() => state
            .triggers_client
            .test_trigger(session.tenant_id, trigger_id, group_key)
            .await
            .ok()
            .map(|r| TestResultView {
                group_key: group_key.clone(),
                would_fire: r.would_fire,
                contributing_record_count: r.contributing_record_count,
            }),
        _ => None,
    };

    let page = query.page.max(0);
    match state
        .triggers_client
        .list_triggers(session.tenant_id, DEFAULT_PAGE_SIZE, page * DEFAULT_PAGE_SIZE)
        .await
    {
        Ok(result) => {
            let mut triggers: Vec<TriggerSummary> =
                result.triggers.into_iter().filter(|t| matches_query(t, &query.q)).collect();
            sort_rows(&mut triggers, &query.sort, &query.dir);
            Html(
                TriggersTemplate {
                    show_nav: true,
                    is_admin,
                    triggers,
                    page,
                    has_more: result.has_more,
                    can_write,
                    error: None,
                    form_error: None,
                    test_result,
                    q: query.q,
                    sort: query.sort,
                    dir: query.dir,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(e) => Html(
            TriggersTemplate {
                show_nav: true,
                is_admin,
                triggers: vec![],
                page,
                has_more: false,
                can_write,
                error: Some(e.to_string()),
                form_error: None,
                test_result,
                q: query.q,
                sort: query.sort,
                dir: query.dir,
            }
            .render()
            .unwrap(),
        )
        .into_response(),
    }
}

/// The create form only ever asks for one condition shape at a time — `condition_shape`
/// picks which of the two field groups below actually gets used; the other group's inputs
/// are ignored even if the browser submitted them (e.g. left at their defaults).
/// Numeric/optional fields arrive as empty strings (not absent keys) from an HTML form that
/// shows both condition shapes' inputs at once with the unused ones left blank — `Option<u32>`
/// etc. would reject `""` as an invalid number, so every field the browser might leave blank
/// is a plain `String`/`Option<String>`, parsed by hand in `build_condition`.
#[derive(Debug, serde::Deserialize)]
pub struct PostTriggerForm {
    name: String,
    event_type_match: String,
    window_seconds: i64,
    condition_shape: String,
    count: String,
    field: String,
    threshold: String,
    direction: Option<String>,
    action_url: Option<String>,
    // ADR-0027: a correlated trigger's legs — event/chat was just the illustrative example in
    // the ADR, this is generic to any event types. Up to 6 (event_type, min_count) pairs — a
    // form can't submit a truly variable-length list without JS, so this is a fixed number of
    // rows (2 shown by default, up to 4 more revealed via the page's "+ Add another source"
    // button — client-side reveal only, ADR-0014's no-JS-by-default stance intact since the
    // rows themselves are still plain server-rendered inputs). Any row with a blank event_type
    // is skipped, not an error. `#[serde(default)]` so existing/hand-built form submissions
    // that predate this shape, or that only fill in fewer rows, still deserialize.
    #[serde(default)]
    correlated_event_type_1: String,
    #[serde(default)]
    correlated_min_count_1: String,
    #[serde(default)]
    correlated_event_type_2: String,
    #[serde(default)]
    correlated_min_count_2: String,
    #[serde(default)]
    correlated_event_type_3: String,
    #[serde(default)]
    correlated_min_count_3: String,
    #[serde(default)]
    correlated_event_type_4: String,
    #[serde(default)]
    correlated_min_count_4: String,
    #[serde(default)]
    correlated_event_type_5: String,
    #[serde(default)]
    correlated_min_count_5: String,
    #[serde(default)]
    correlated_event_type_6: String,
    #[serde(default)]
    correlated_min_count_6: String,
}

fn build_correlated_conditions(
    form: &PostTriggerForm,
) -> Result<Vec<common::CorrelatedCondition>, &'static str> {
    let rows = [
        (&form.correlated_event_type_1, &form.correlated_min_count_1),
        (&form.correlated_event_type_2, &form.correlated_min_count_2),
        (&form.correlated_event_type_3, &form.correlated_min_count_3),
        (&form.correlated_event_type_4, &form.correlated_min_count_4),
        (&form.correlated_event_type_5, &form.correlated_min_count_5),
        (&form.correlated_event_type_6, &form.correlated_min_count_6),
    ];
    let mut conditions = Vec::new();
    for (event_type, min_count) in rows {
        let event_type = event_type.trim();
        if event_type.is_empty() {
            continue;
        }
        let min_count: u32 = min_count
            .trim()
            .parse()
            .map_err(|_| "each correlated row with an event type needs a valid min count")?;
        conditions
            .push(common::CorrelatedCondition { event_type: event_type.to_string(), min_count });
    }
    if conditions.is_empty() {
        return Err("at least one correlated event type/min count row is required");
    }
    Ok(conditions)
}

fn build_condition(form: &PostTriggerForm) -> Result<TriggerCondition, &'static str> {
    match form.condition_shape.as_str() {
        "count_over_window" => {
            let count: u32 = form
                .count
                .trim()
                .parse()
                .map_err(|_| "count is required for this condition shape")?;
            Ok(TriggerCondition::CountOverWindow { count })
        }
        "threshold_over_window" => {
            let field = form.field.trim();
            if field.is_empty() {
                return Err("field is required for this condition shape");
            }
            let threshold: f64 = form
                .threshold
                .trim()
                .parse()
                .map_err(|_| "threshold is required for this condition shape")?;
            let direction = match form.direction.as_deref() {
                Some("below") => ThresholdDirection::Below,
                _ => ThresholdDirection::Above,
            };
            Ok(TriggerCondition::ThresholdOverWindow {
                field: field.to_string(),
                threshold,
                direction,
            })
        }
        "correlated_over_window" => Ok(TriggerCondition::CorrelatedOverWindow {
            conditions: build_correlated_conditions(form)?,
        }),
        _ => Err("unknown condition shape"),
    }
}

pub async fn post_trigger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PostTriggerForm>,
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

    let condition = match build_condition(&form) {
        Ok(c) => c,
        Err(msg) => {
            let result = state
                .triggers_client
                .list_triggers(session.tenant_id, DEFAULT_PAGE_SIZE, 0)
                .await
                .unwrap_or(crate::TriggersPage { triggers: vec![], has_more: false });
            return Html(
                TriggersTemplate {
                    show_nav: true,
                    is_admin,
                    triggers: result.triggers,
                    page: 0,
                    has_more: result.has_more,
                    can_write,
                    error: None,
                    form_error: Some(msg.to_string()),
                    test_result: None,
                    q: String::new(),
                    sort: String::new(),
                    dir: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response();
        }
    };

    let actions = match form.action_url.filter(|u| !u.is_empty()) {
        Some(url) => vec![common::ActionRef {
            action_type: common::ActionType::Webhook,
            config: serde_json::json!({"url": url}),
        }],
        None => vec![],
    };

    // ADR-0027: a CorrelatedOverWindow trigger's `event_type_match` is just a display/audit
    // label, set to its first listed condition's event type — lookup for that shape goes
    // through the correlated `conditions` list, not this field.
    let event_type_match = match &condition {
        TriggerCondition::CorrelatedOverWindow { conditions } => conditions[0].event_type.clone(),
        _ => form.event_type_match,
    };

    let trigger = TriggerDefinition {
        id: uuid::Uuid::new_v4(),
        tenant_id: session.tenant_id,
        name: form.name,
        event_type_match,
        condition,
        window_seconds: form.window_seconds,
        actions,
        enabled: true,
    };

    match state.triggers_client.create_trigger(session.role, &session.username, trigger).await {
        Ok(_) => Redirect::to("/triggers").into_response(),
        Err(e) => {
            let result = state
                .triggers_client
                .list_triggers(session.tenant_id, DEFAULT_PAGE_SIZE, 0)
                .await
                .unwrap_or(crate::TriggersPage { triggers: vec![], has_more: false });
            Html(
                TriggersTemplate {
                    show_nav: true,
                    is_admin,
                    triggers: result.triggers,
                    page: 0,
                    has_more: result.has_more,
                    can_write,
                    error: None,
                    form_error: Some(e.to_string()),
                    test_result: None,
                    q: String::new(),
                    sort: String::new(),
                    dir: String::new(),
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    }
}
