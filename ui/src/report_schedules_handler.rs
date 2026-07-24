use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use common::ReportRun;
use common::SavedSearchQuery;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Default)]
pub struct ScheduleQuery {
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ScheduleFilter {
    #[serde(default)]
    view_kind: String,
    #[serde(default)]
    frequency: String,
    #[serde(default)]
    recipient: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default = "default_format")]
    format: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}
fn default_format() -> String {
    "csv".into()
}

fn run_notice(status: &str) -> &'static str {
    if matches!(status, "generated" | "delivered") {
        "run-complete"
    } else {
        "run-failed"
    }
}

struct ScheduleView {
    id: Uuid,
    name: String,
    frequency: String,
    recipient: String,
    from: String,
    to: String,
    format: String,
    enabled: bool,
    export_url: String,
}

struct RunView {
    id: Uuid,
    schedule_id: Uuid,
    schedule_name: String,
    recipient: String,
    status: String,
    format: String,
    artifact_url: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "report_schedules.html")]
struct ReportSchedulesTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    schedules: Vec<ScheduleView>,
    active_count: usize,
    runs: Vec<RunView>,
    error: Option<String>,
    notice: String,
    successful_run_count: usize,
    failed_run_count: usize,
    in_flight_run_count: usize,
    pdf_schedule_count: usize,
    csv_schedule_count: usize,
    status: String,
}

fn normalize_run_status(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "success" | "generated" | "delivered" => "success".to_string(),
        "failed" | "delivery_failed" => "failed".to_string(),
        "running" | "in_progress" => "running".to_string(),
        _ => String::new(),
    }
}

fn matches_run_status(run: &RunView, status: &str) -> bool {
    status.is_empty() || normalize_run_status(&run.status) == status
}

fn schedule_views(queries: Vec<SavedSearchQuery>) -> Vec<ScheduleView> {
    queries
        .into_iter()
        .filter_map(|query| {
            let filter: ScheduleFilter = serde_json::from_value(query.filter).ok()?;
            if filter.view_kind != "report_schedule" {
                return None;
            }
            let mut params = vec![("from", filter.from.clone()), ("to", filter.to.clone())];
            params.retain(|(_, value)| !value.is_empty());
            let extension = if filter.format == "pdf" { "pdf" } else { "csv" };
            Some(ScheduleView {
                id: query.id,
                name: query.name,
                frequency: filter.frequency,
                recipient: filter.recipient,
                from: filter.from,
                to: filter.to,
                format: filter.format,
                enabled: filter.enabled,
                export_url: format!(
                    "/reports/export.{extension}?{}",
                    serde_urlencoded::to_string(params).unwrap_or_default()
                ),
            })
        })
        .collect()
}

pub async fn get_report_schedules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let queries = match state.saved_search_queries_client.list(session.tenant_id).await {
        Ok(queries) => queries,
        Err(error) => {
            return Html(
                ReportSchedulesTemplate {
                    show_nav: true,
                    is_admin: session.role.at_least(common::Role::Admin),
                    can_write: session.role.at_least(common::Role::Operator),
                    schedules: vec![],
                    active_count: 0,
                    runs: vec![],
                    error: Some(error.to_string()),
                    notice: query.notice,
                    successful_run_count: 0,
                    failed_run_count: 0,
                    in_flight_run_count: 0,
                    pdf_schedule_count: 0,
                    csv_schedule_count: 0,
                    status: query.status,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    };
    let schedules = schedule_views(queries);
    let active_count = schedules.iter().filter(|schedule| schedule.enabled).count();
    let runs: Vec<RunView> = state
        .saved_search_queries_client
        .list_report_runs(session.tenant_id, None)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|run| RunView {
            id: run.id,
            schedule_id: run.schedule_id,
            schedule_name: run.schedule_name,
            recipient: run.recipient,
            status: run.status,
            format: run.format,
            artifact_url: run.artifact_url,
            started_at: run.started_at,
            completed_at: run.completed_at,
            error: run.error,
        })
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|run| matches_run_status(run, &query.status))
        .collect();
    let successful_run_count =
        runs.iter().filter(|run| matches!(run.status.as_str(), "generated" | "delivered")).count();
    let failed_run_count = runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "failed" | "delivery_failed"))
        .count();
    let in_flight_run_count = runs.len().saturating_sub(successful_run_count + failed_run_count);
    let pdf_schedule_count = schedules.iter().filter(|schedule| schedule.format == "pdf").count();
    let csv_schedule_count = schedules.iter().filter(|schedule| schedule.format != "pdf").count();
    Html(
        ReportSchedulesTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            can_write: session.role.at_least(common::Role::Operator),
            schedules,
            active_count,
            runs,
            error: None,
            notice: query.notice,
            successful_run_count,
            failed_run_count,
            in_flight_run_count,
            pdf_schedule_count,
            csv_schedule_count,
            status: query.status,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleForm {
    name: String,
    frequency: String,
    recipient: String,
    from: String,
    to: String,
    #[serde(default = "default_format")]
    format: String,
}

fn valid_frequency(value: &str) -> bool {
    matches!(value, "daily" | "weekly" | "monthly")
}

pub async fn post_create_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CreateScheduleForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if form.name.trim().is_empty()
        || !valid_frequency(form.frequency.trim())
        || !form.recipient.contains('@')
        || !matches!(form.format.trim(), "csv" | "pdf")
    {
        return Redirect::to("/reports/schedules?notice=invalid").into_response();
    }
    let filter = ScheduleFilter {
        view_kind: "report_schedule".into(),
        frequency: form.frequency.trim().into(),
        recipient: form.recipient.trim().into(),
        from: form.from,
        to: form.to,
        format: form.format.trim().into(),
        enabled: true,
    };
    match state
        .saved_search_queries_client
        .create(
            session.tenant_id,
            form.name.trim(),
            serde_json::to_value(filter).unwrap_or_default(),
        )
        .await
    {
        Ok(_) => Redirect::to("/reports/schedules?notice=created").into_response(),
        Err(_) => Redirect::to("/reports/schedules?notice=failed").into_response(),
    }
}

pub async fn post_delete_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let _ = state.saved_search_queries_client.delete(session.tenant_id, id).await;
    Redirect::to("/reports/schedules?notice=deleted").into_response()
}

pub async fn post_toggle_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let queries =
        state.saved_search_queries_client.list(session.tenant_id).await.unwrap_or_default();
    if let Some(query) = queries.into_iter().find(|query| query.id == id) {
        let mut filter: ScheduleFilter = serde_json::from_value(query.filter).unwrap_or_default();
        if filter.view_kind == "report_schedule" {
            filter.enabled = !filter.enabled;
            let _ = state.saved_search_queries_client.delete(session.tenant_id, id).await;
            let _ = state
                .saved_search_queries_client
                .create(
                    session.tenant_id,
                    &query.name,
                    serde_json::to_value(filter).unwrap_or_default(),
                )
                .await;
        }
    }
    Redirect::to("/reports/schedules?notice=toggled").into_response()
}

pub async fn post_run_report_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let query = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|query| query.id == id);
    let Some(query) = query else {
        return Redirect::to("/reports/schedules?notice=run-failed").into_response();
    };
    let filter: ScheduleFilter = serde_json::from_value(query.filter).unwrap_or_default();
    if filter.view_kind != "report_schedule" {
        return Redirect::to("/reports/schedules?notice=run-failed").into_response();
    }
    let params =
        serde_urlencoded::to_string([("from", filter.from.clone()), ("to", filter.to.clone())])
            .unwrap_or_default();
    let extension = if filter.format == "pdf" { "pdf" } else { "csv" };
    let mut run = ReportRun::new(
        session.tenant_id,
        id,
        query.name,
        filter.recipient,
        format!("/reports/export.{extension}?{params}"),
    );
    run.format = filter.format;
    if state.saved_search_queries_client.create_report_run(session.role, run.clone()).await.is_err()
    {
        return Redirect::to("/reports/schedules?notice=run-failed").into_response();
    }
    let (since, until) =
        (parse_schedule_date(&filter.from, false), parse_schedule_date(&filter.to, true));
    match state.events_client.list_events(&session.bearer_token, 1000, 0, since, until).await {
        Ok(_) => {
            run.status = "generated".into();
            run.completed_at = Some(chrono::Utc::now());
        }
        Err(error) => {
            run.status = "failed".into();
            run.error = Some(error.to_string());
            run.artifact_url = None;
            run.completed_at = Some(chrono::Utc::now());
        }
    }
    let notice = run_notice(&run.status);
    if state.saved_search_queries_client.update_report_run(session.role, run).await.is_err() {
        return Redirect::to("/reports/schedules?notice=run-failed").into_response();
    }
    Redirect::to(&format!("/reports/schedules?notice={notice}")).into_response()
}

#[cfg(test)]
mod report_schedules_handler_test {
    #[test]
    fn schedule_run_exposes_delivery_preflight() {
        let template = include_str!("../templates/report_schedules.html");
        assert!(template.contains("schedule-run-preflight"));
        assert!(template.contains("Run preflight:"));
        assert!(template.contains("artifact recorded in run history."));
    }
}

fn parse_schedule_date(value: &str, end: bool) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| {
            date.and_hms_opt(
                if end { 23 } else { 0 },
                if end { 59 } else { 0 },
                if end { 59 } else { 0 },
            )
        })
        .map(|date| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(date, chrono::Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_report_schedule_views_are_projected() {
        let tenant = Uuid::new_v4();
        let schedule = SavedSearchQuery::new(
            tenant,
            "weekly",
            serde_json::json!({"view_kind":"report_schedule","frequency":"weekly","recipient":"ops@example.com","enabled":true}),
        );
        let ordinary = SavedSearchQuery::new(
            tenant,
            "saved report",
            serde_json::json!({"view_kind":"reports"}),
        );
        assert_eq!(schedule_views(vec![schedule, ordinary]).len(), 1);
    }

    #[test]
    fn report_run_notice_matches_persisted_outcome() {
        assert_eq!(run_notice("generated"), "run-complete");
        assert_eq!(run_notice("delivered"), "run-complete");
        assert_eq!(run_notice("failed"), "run-failed");
        assert_eq!(run_notice("delivery_failed"), "run-failed");
    }

    #[test]
    fn run_outcome_filters_normalize_delivery_states() {
        let run = RunView {
            id: Uuid::new_v4(),
            schedule_id: Uuid::new_v4(),
            schedule_name: "weekly".into(),
            recipient: "ops@example.com".into(),
            status: "delivery_failed".into(),
            format: "pdf".into(),
            artifact_url: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            error: Some("relay unavailable".into()),
        };
        assert!(matches_run_status(&run, "failed"));
        assert!(!matches_run_status(&run, "success"));
        assert_eq!(normalize_run_status("generated"), "success");
    }
}
