#[path = "incident_correlation_handler_test.rs"]
#[cfg(test)]
mod incident_correlation_handler_test;

#[path = "incident_correlation_model.rs"]
mod incident_correlation_model;
#[path = "incident_correlation_principal.rs"]
mod incident_correlation_principal;
pub(crate) use incident_correlation_model::{
    build_ambiguous_correlation_groups, build_correlation_candidates, AmbiguousCorrelationGroup,
    CorrelationCandidate, CorrelationIncidentOption,
};

use crate::session_guard::require_session;
use crate::{AppState, EventSummary};
use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use incident_correlation_principal::{require_api_principal, ConsolePrincipal};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

async fn list_all_events(
    state: &AppState,
    bearer_token: &str,
) -> Result<Vec<EventSummary>, String> {
    let mut events = Vec::new();
    let mut offset = 0u32;
    for _ in 0..20 {
        let page = state
            .events_client
            .list_events(bearer_token, 1000, offset, None, None)
            .await
            .map_err(|error| error.to_string())?;
        let page_len = page.events.len() as u32;
        events.extend(page.events);
        if !page.has_more || page_len == 0 {
            break;
        }
        offset = offset.saturating_add(page_len);
    }
    Ok(events)
}

async fn load_correlation_candidates(
    state: &AppState,
    principal: &ConsolePrincipal,
) -> Result<Vec<CorrelationCandidate>, String> {
    Ok(load_correlation_review(state, principal).await?.0)
}

async fn load_correlation_review(
    state: &AppState,
    principal: &ConsolePrincipal,
) -> Result<(Vec<CorrelationCandidate>, Vec<AmbiguousCorrelationGroup>), String> {
    let incidents = state
        .incidents_client
        .list_incidents(principal.tenant_id, None)
        .await
        .map_err(|error| error.to_string())?;
    let events = list_all_events(state, &principal.bearer_token).await?;
    Ok((
        build_correlation_candidates(&incidents, &events),
        build_ambiguous_correlation_groups(&incidents, &events),
    ))
}

async fn apply_selected_candidates(
    state: &AppState,
    principal: &ConsolePrincipal,
    selected: &HashSet<Uuid>,
) -> Result<(usize, usize, usize), String> {
    let candidates = load_correlation_candidates(state, principal).await?;
    let candidate_ids =
        candidates.iter().map(|candidate| candidate.event_id).collect::<HashSet<_>>();
    let rejected_count = selected.difference(&candidate_ids).count();
    let mut linked_count = 0usize;
    let mut failed_count = 0usize;
    for candidate in
        candidates.into_iter().filter(|candidate| selected.contains(&candidate.event_id))
    {
        match state
            .incidents_client
            .link_event_with_context(
                principal.role,
                &principal.username,
                principal.tenant_id,
                candidate.incident_id,
                candidate.event_id,
                &candidate.group_key,
            )
            .await
        {
            Ok(()) => linked_count += 1,
            Err(_) => failed_count += 1,
        }
    }
    Ok((linked_count, failed_count, rejected_count))
}

async fn apply_all_candidates(
    state: &AppState,
    principal: &ConsolePrincipal,
) -> Result<(usize, usize, usize), String> {
    let candidates = load_correlation_candidates(state, principal).await?;
    let selected = candidates.iter().map(|candidate| candidate.event_id).collect::<HashSet<_>>();
    let requested_count = selected.len();
    if selected.is_empty() {
        return Ok((0, 0, 0));
    }
    let (linked_count, failed_count, _rejected_count) =
        apply_selected_candidates(state, principal, &selected).await?;
    // The candidate set is freshly loaded inside apply_selected_candidates as well. A concurrent
    // change can therefore make an item disappear; report that as rejected rather than claiming
    // every initially selected signal was linked.
    Ok((linked_count, failed_count, requested_count.saturating_sub(linked_count + failed_count)))
}

async fn apply_ambiguous_resolution(
    state: &AppState,
    principal: &ConsolePrincipal,
    form: &CorrelationResolveForm,
) -> Result<(usize, usize), String> {
    let (_, groups) = load_correlation_review(state, principal).await?;
    let group_key = form.group_key.trim().to_ascii_lowercase();
    let group = groups
        .iter()
        .find(|group| group.group_key == group_key)
        .ok_or_else(|| "the ambiguity group is no longer available".to_string())?;
    if !group.incident_options.iter().any(|option| option.id == form.incident_id) {
        return Err("the selected incident is not an active target for this group".to_string());
    }
    let selected = form.event_ids.iter().copied().collect::<HashSet<_>>();
    if selected.is_empty() {
        return Err("select at least one signal to resolve".to_string());
    }
    if selected.iter().any(|event_id| !group.event_ids.contains(event_id)) {
        return Err("one or more selected signals are outside the ambiguity group".to_string());
    }
    let mut linked_count = 0usize;
    let mut failed_count = 0usize;
    for event_id in selected {
        match state
            .incidents_client
            .link_event_with_context(
                principal.role,
                &principal.username,
                principal.tenant_id,
                form.incident_id,
                event_id,
                &group.group_key,
            )
            .await
        {
            Ok(()) => linked_count += 1,
            Err(_) => failed_count += 1,
        }
    }
    Ok((linked_count, failed_count))
}

#[derive(Debug, Deserialize, Default)]
pub struct CorrelationSweepQuery {
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub linked_count: usize,
    #[serde(default)]
    pub failed_count: usize,
}

#[derive(Debug, Deserialize, Default)]
pub struct CorrelationSweepForm {
    #[serde(default)]
    pub event_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CorrelationResolveForm {
    pub incident_id: Uuid,
    pub group_key: String,
    #[serde(default)]
    pub event_ids: Vec<Uuid>,
}

struct CorrelationCandidateView {
    event_id: Uuid,
    incident_id: Uuid,
    incident_title: String,
    group_key: String,
    event_type: String,
    occurred_at: String,
    confidence: String,
    evidence: Vec<String>,
}

struct AmbiguousCorrelationGroupView {
    group_key: String,
    event_ids: Vec<Uuid>,
    incident_options: Vec<CorrelationIncidentOption>,
}

#[derive(Template)]
#[template(path = "incident_correlation_sweep.html")]
struct CorrelationSweepTemplate {
    show_nav: bool,
    is_admin: bool,
    can_write: bool,
    candidates: Vec<CorrelationCandidateView>,
    ambiguous_groups: Vec<AmbiguousCorrelationGroupView>,
    notice: String,
    linked_count: usize,
    failed_count: usize,
    error: Option<String>,
}

fn render_sweep(
    session: &crate::Session,
    candidates: Vec<CorrelationCandidate>,
    ambiguous_groups: Vec<AmbiguousCorrelationGroup>,
    notice: String,
    linked_count: usize,
    failed_count: usize,
    error: Option<String>,
) -> Response {
    let candidates = candidates
        .into_iter()
        .map(|candidate| CorrelationCandidateView {
            event_id: candidate.event_id,
            incident_id: candidate.incident_id,
            incident_title: candidate.incident_title,
            group_key: candidate.group_key,
            event_type: candidate.event_type,
            occurred_at: candidate.occurred_at,
            confidence: candidate.confidence,
            evidence: candidate.evidence,
        })
        .collect();
    let ambiguous_groups = ambiguous_groups
        .into_iter()
        .map(|group| AmbiguousCorrelationGroupView {
            group_key: group.group_key,
            event_ids: group.event_ids,
            incident_options: group.incident_options,
        })
        .collect();
    Html(
        CorrelationSweepTemplate {
            show_nav: true,
            is_admin: session.role.at_least(common::Role::Admin),
            can_write: session.role.at_least(common::Role::Operator),
            candidates,
            ambiguous_groups,
            notice,
            linked_count,
            failed_count,
            error,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

pub async fn get_correlation_sweep(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CorrelationSweepQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let principal = ConsolePrincipal::from(&session);
    let (candidates, ambiguous_groups) = match load_correlation_review(&state, &principal).await {
        Ok(review) => review,
        Err(error) => {
            return render_sweep(
                &session,
                vec![],
                vec![],
                query.notice,
                query.linked_count,
                query.failed_count,
                Some(error),
            )
        }
    };
    render_sweep(
        &session,
        candidates,
        ambiguous_groups,
        query.notice,
        query.linked_count,
        query.failed_count,
        None,
    )
}

pub async fn post_correlation_sweep(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CorrelationSweepForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if form.event_ids.is_empty() {
        return Redirect::to("/incidents/correlation-sweep?notice=empty").into_response();
    }
    let selected = form.event_ids.into_iter().collect::<HashSet<_>>();
    let (linked_count, failed_count, _) =
        match apply_selected_candidates(&state, &ConsolePrincipal::from(&session), &selected).await
        {
            Ok(counts) => counts,
            Err(_) => {
                return Redirect::to("/incidents/correlation-sweep?notice=failed").into_response()
            }
        };
    Redirect::to(&format!(
        "/incidents/correlation-sweep?notice=applied&linked_count={linked_count}&failed_count={failed_count}"
    ))
    .into_response()
}

pub async fn post_correlation_sweep_auto(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let principal = ConsolePrincipal::from(&session);
    let (linked_count, failed_count, _) = match apply_all_candidates(&state, &principal).await {
        Ok(counts) => counts,
        Err(_) => {
            return Redirect::to("/incidents/correlation-sweep?notice=failed").into_response()
        }
    };
    Redirect::to(&format!(
        "/incidents/correlation-sweep?notice=auto-applied&linked_count={linked_count}&failed_count={failed_count}"
    ))
    .into_response()
}

pub async fn post_correlation_sweep_resolve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CorrelationResolveForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if !session.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    match apply_ambiguous_resolution(&state, &ConsolePrincipal::from(&session), &form).await {
        Ok((linked_count, failed_count)) => Redirect::to(&format!(
            "/incidents/correlation-sweep?notice=resolved&linked_count={linked_count}&failed_count={failed_count}"
        ))
        .into_response(),
        Err(_) => Redirect::to("/incidents/correlation-sweep?notice=resolution-failed").into_response(),
    }
}

#[derive(Debug, Serialize)]
struct CorrelationSweepApiResponse {
    candidates: Vec<CorrelationCandidate>,
    ambiguous_groups: Vec<AmbiguousCorrelationGroup>,
    candidate_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct CorrelationSweepApiRequest {
    #[serde(default)]
    pub event_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
struct CorrelationSweepApiApplyResponse {
    requested_count: usize,
    linked_count: usize,
    failed_count: usize,
    rejected_count: usize,
}

#[derive(Debug, Serialize)]
struct CorrelationSweepApiError {
    error: String,
}

pub async fn get_correlation_sweep_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match require_api_principal(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    match load_correlation_review(&state, &principal).await {
        Ok((candidates, ambiguous_groups)) => axum::Json(CorrelationSweepApiResponse {
            candidate_count: candidates.len(),
            candidates,
            ambiguous_groups,
        })
        .into_response(),
        Err(error) => {
            (axum::http::StatusCode::BAD_GATEWAY, axum::Json(CorrelationSweepApiError { error }))
                .into_response()
        }
    }
}

pub async fn post_correlation_sweep_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CorrelationSweepApiRequest>,
) -> Response {
    let principal = match require_api_principal(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if !principal.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let selected = request.event_ids.into_iter().collect::<HashSet<_>>();
    if selected.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(CorrelationSweepApiError {
                error: "event_ids must contain at least one candidate signal".to_string(),
            }),
        )
            .into_response();
    }
    match apply_selected_candidates(&state, &principal, &selected).await {
        Ok((linked_count, failed_count, rejected_count)) => {
            axum::Json(CorrelationSweepApiApplyResponse {
                requested_count: selected.len(),
                linked_count,
                failed_count,
                rejected_count,
            })
            .into_response()
        }
        Err(error) => {
            (axum::http::StatusCode::BAD_GATEWAY, axum::Json(CorrelationSweepApiError { error }))
                .into_response()
        }
    }
}

/// Applies every currently safe candidate in one bounded, deterministic sweep. The candidate
/// query is re-run immediately before each link batch, so ambiguous keys and cases that changed
/// during the request are never guessed at or silently reassigned.
pub async fn post_correlation_sweep_auto_api(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let principal = match require_api_principal(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if !principal.role.at_least(common::Role::Operator) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    match apply_all_candidates(&state, &principal).await {
        Ok((linked_count, failed_count, rejected_count)) => {
            axum::Json(CorrelationSweepApiApplyResponse {
                requested_count: linked_count + failed_count + rejected_count,
                linked_count,
                failed_count,
                rejected_count,
            })
            .into_response()
        }
        Err(error) => {
            (axum::http::StatusCode::BAD_GATEWAY, axum::Json(CorrelationSweepApiError { error }))
                .into_response()
        }
    }
}
