use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::{ActionRef, ActionType, TriggerCondition, TriggerDefinition};
use uuid::Uuid;

#[derive(Debug, serde::Deserialize, Default)]
pub struct TriggerDetailQuery {
    #[serde(default)]
    pub test_group_key: String,
    #[serde(default)]
    pub notice: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct EditTriggerForm {
    pub name: String,
    pub event_type_match: String,
    pub window_seconds: i64,
    pub condition: String,
    pub actions: String,
}

struct TriggerActionView {
    provider: String,
    config: String,
}

struct TriggerConditionView {
    shape: String,
    summary: String,
    detail: String,
}

struct TriggerActivityBar {
    date: String,
    count: usize,
    height_pct: usize,
    href: String,
}

#[derive(Template)]
#[template(path = "trigger_detail.html")]
struct TriggerDetailTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    trigger: Option<TriggerDefinition>,
    condition: Option<TriggerConditionView>,
    actions: Vec<TriggerActionView>,
    config_pretty: String,
    condition_json: String,
    actions_json: String,
    test_group_key: String,
    test_would_fire: Option<bool>,
    test_record_count: Option<usize>,
    activity: Vec<TriggerActivityBar>,
    notice: String,
    error: Option<String>,
}

fn action_name(action: ActionType) -> &'static str {
    match action {
        ActionType::Email => "Email",
        ActionType::Webhook => "Webhook / HTTP relay",
        ActionType::TeamsAlert => "Microsoft Teams alert",
        ActionType::CreateTicket => "Create ticket",
        ActionType::Custom => "Custom provider",
    }
}

fn condition_view(condition: &TriggerCondition) -> TriggerConditionView {
    match condition {
        TriggerCondition::CountOverWindow { count } => TriggerConditionView {
            shape: "Count over window".to_string(),
            summary: format!("At least {count} matching signal(s)"),
            detail: "Every matching signal is grouped by entity and evaluated within the configured window.".to_string(),
        },
        TriggerCondition::ThresholdOverWindow { field, threshold, direction } => TriggerConditionView {
            shape: "Threshold over window".to_string(),
            summary: format!("{field} {} {threshold}", match direction { common::ThresholdDirection::Above => "above", common::ThresholdDirection::Below => "below" }),
            detail: "The trigger fires when a numeric field crosses the configured threshold for the entity.".to_string(),
        },
        TriggerCondition::CorrelatedOverWindow { conditions } => TriggerConditionView {
            shape: "Correlated sources".to_string(),
            summary: format!("{} source condition(s) must be satisfied", conditions.len()),
            detail: conditions.iter().map(|item| format!("{} ≥ {}", item.event_type, item.min_count)).collect::<Vec<_>>().join(" · "),
        },
    }
}

fn config_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn error_template(is_admin: bool, can_write: bool, error: String) -> Response {
    Html(
        TriggerDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            trigger: None,
            condition: None,
            actions: vec![],
            config_pretty: String::new(),
            condition_json: String::new(),
            actions_json: String::new(),
            test_group_key: String::new(),
            test_would_fire: None,
            test_record_count: None,
            activity: vec![],
            notice: String::new(),
            error: Some(error),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn get_trigger_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<TriggerDetailQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let can_write = session.role.at_least(common::Role::Operator);
    let trigger = match state.triggers_client.get_trigger(session.tenant_id, id).await {
        Ok(Some(trigger)) => trigger,
        Ok(None) => {
            return error_template(is_admin, can_write, "no trigger found with this id".to_string())
        }
        Err(error) => return error_template(is_admin, can_write, error.to_string()),
    };
    let test = if query.test_group_key.trim().is_empty() {
        None
    } else {
        state
            .triggers_client
            .test_trigger(session.tenant_id, id, query.test_group_key.trim())
            .await
            .ok()
    };
    let actions = trigger
        .actions
        .iter()
        .map(|action| TriggerActionView {
            provider: action_name(action.action_type).to_string(),
            config: config_pretty(&action.config),
        })
        .collect();
    let condition = Some(condition_view(&trigger.condition));
    let activity = trigger_activity(&state, &session.bearer_token, &trigger).await;
    Html(
        TriggerDetailTemplate {
            show_nav: true,
            is_admin,
            can_write,
            config_pretty: serde_json::json!({
                "event_type_match": &trigger.event_type_match,
                "condition": &trigger.condition,
                "window_seconds": trigger.window_seconds,
                "actions": &trigger.actions,
            })
            .to_string(),
            condition_json: config_pretty(
                &serde_json::to_value(&trigger.condition).unwrap_or_default(),
            ),
            actions_json: config_pretty(
                &serde_json::to_value(&trigger.actions).unwrap_or_default(),
            ),
            condition,
            actions,
            test_group_key: query.test_group_key,
            test_would_fire: test.as_ref().map(|result| result.would_fire),
            test_record_count: test.map(|result| result.contributing_record_count),
            activity,
            notice: query.notice,
            trigger: Some(trigger),
            error: None,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

async fn trigger_activity(
    state: &AppState,
    bearer_token: &str,
    trigger: &TriggerDefinition,
) -> Vec<TriggerActivityBar> {
    let page = match state.events_client.list_events(bearer_token, 1000, 0, None, None).await {
        Ok(page) => page,
        Err(_) => return vec![],
    };
    let mut buckets = std::collections::BTreeMap::<String, usize>::new();
    for event in page.events {
        if !trigger.event_type_match.trim().is_empty()
            && trigger.event_type_match != "*"
            && !event.event_type.eq_ignore_ascii_case(&trigger.event_type_match)
        {
            continue;
        }
        *buckets.entry(event.occurred_at.format("%Y-%m-%d").to_string()).or_default() += 1;
    }
    let max = buckets.values().copied().max().unwrap_or(1);
    buckets
        .into_iter()
        .map(|(date, count)| {
            let query = serde_urlencoded::to_string([
                ("from", date.clone()),
                ("to", date.clone()),
                ("q", trigger.event_type_match.clone()),
            ])
            .unwrap_or_default();
            TriggerActivityBar {
                date,
                count,
                height_pct: (count * 100 / max).max(8),
                href: format!("/events?{query}"),
            }
        })
        .collect()
}

pub async fn post_trigger_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<EditTriggerForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let Ok(Some(mut trigger)) = state.triggers_client.get_trigger(session.tenant_id, id).await
    else {
        return Redirect::to(&format!("/triggers/{id}?notice=edit-failed")).into_response();
    };
    let condition = match serde_json::from_str::<TriggerCondition>(&form.condition) {
        Ok(value) => value,
        Err(_) => {
            return Redirect::to(&format!("/triggers/{id}?notice=invalid-condition"))
                .into_response()
        }
    };
    let actions = match serde_json::from_str::<Vec<ActionRef>>(&form.actions) {
        Ok(value) => value,
        Err(_) => {
            return Redirect::to(&format!("/triggers/{id}?notice=invalid-actions")).into_response()
        }
    };
    if form.name.trim().is_empty()
        || form.event_type_match.trim().is_empty()
        || form.window_seconds < 1
    {
        return Redirect::to(&format!("/triggers/{id}?notice=invalid-fields")).into_response();
    }
    trigger.name = form.name.trim().to_string();
    trigger.event_type_match = form.event_type_match.trim().to_string();
    trigger.window_seconds = form.window_seconds;
    trigger.condition = condition;
    trigger.actions = actions;
    match state.triggers_client.update_trigger(session.role, &session.username, trigger).await {
        Ok(_) => Redirect::to(&format!("/triggers/{id}?notice=updated")).into_response(),
        Err(_) => Redirect::to(&format!("/triggers/{id}?notice=edit-failed")).into_response(),
    }
}
