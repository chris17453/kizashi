use crate::ingestion_stats_client::RecordSearchFilter;
use crate::ontology_client;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use common::SavedSearchQuery;

#[derive(Debug, serde::Deserialize, Default)]
pub struct GlobalSearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub notice: String,
}

struct RecordHit {
    id: uuid::Uuid,
    title: String,
    source_type: String,
    connector_id: String,
    ingested_at: String,
}

struct SensorHit {
    id: uuid::Uuid,
    name: String,
    connector_type: String,
    enabled: bool,
}

struct IdentityHit {
    id: uuid::Uuid,
    username: String,
    role: String,
    mfa_enabled: bool,
}

struct EntityHit {
    id: uuid::Uuid,
    type_name: String,
    title: String,
    state: String,
    related_count: usize,
    action_count: usize,
    signal_count: usize,
    case_count: usize,
    signals: Vec<EntityContextLink>,
    cases: Vec<EntityContextLink>,
}

struct EntityContextLink {
    id: uuid::Uuid,
    label: String,
}

struct IncidentHit {
    id: uuid::Uuid,
    title: String,
    severity: String,
    status: String,
    event_count: usize,
    event_links: Vec<EntityContextLink>,
}

struct ActionHit {
    id: uuid::Uuid,
    name: String,
    outcome: String,
    review_status: String,
    review_assignee: Option<String>,
    review_stale: bool,
    target_context: String,
    targets: Vec<ActionTargetHit>,
    event_id: Option<uuid::Uuid>,
    incident_id: Option<uuid::Uuid>,
    executed_at: String,
}

struct ActionTargetHit {
    id: uuid::Uuid,
    label: String,
}

struct EventHit {
    id: uuid::Uuid,
    event_type: String,
    group_key: String,
    status: String,
    occurred_at: String,
    record_count: usize,
}

struct AuditHit {
    id: uuid::Uuid,
    entity_id: uuid::Uuid,
    service: String,
    entity_type: String,
    change_type: String,
    actor: String,
    changed_at: String,
}

#[derive(Template)]
#[template(path = "search.html")]
struct SearchTemplate {
    show_nav: bool,
    is_admin: bool,
    query: String,
    scope: String,
    saved_views: Vec<SavedGlobalSearchView>,
    notice: String,
    records: Vec<RecordHit>,
    sensors: Vec<SensorHit>,
    identities: Vec<IdentityHit>,
    entities: Vec<EntityHit>,
    incidents: Vec<IncidentHit>,
    actions: Vec<ActionHit>,
    events: Vec<EventHit>,
    audits: Vec<AuditHit>,
    errors: Vec<String>,
    searched: bool,
}

#[derive(Debug, serde::Deserialize, Default)]
struct SavedGlobalSearchFilter {
    #[serde(default)]
    q: String,
    #[serde(default)]
    scope: String,
}

struct SavedGlobalSearchView {
    id: uuid::Uuid,
    name: String,
    load_url: String,
}

fn to_saved_global_search_view(query: SavedSearchQuery) -> SavedGlobalSearchView {
    let filter: SavedGlobalSearchFilter = serde_json::from_value(query.filter).unwrap_or_default();
    let scope = normalize_scope(&filter.scope);
    let params =
        serde_urlencoded::to_string([("q", filter.q), ("scope", scope)]).unwrap_or_default();
    SavedGlobalSearchView { id: query.id, name: query.name, load_url: format!("/search?{params}") }
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}

fn normalize_scope(scope: &str) -> String {
    match scope.trim().to_ascii_lowercase().as_str() {
        "records" | "entities" | "incidents" | "events" | "actions" | "audit" | "sensors"
        | "identities" => scope.trim().to_ascii_lowercase(),
        _ => "all".to_string(),
    }
}

fn scope_is(scope: &str, category: &str) -> bool {
    scope == "all" || scope == category
}

pub async fn get_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GlobalSearchQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let term = query.q.trim().to_string();
    let scope = normalize_scope(&query.scope).to_string();
    let saved_views = state
        .saved_search_queries_client
        .list(session.tenant_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|saved| {
            saved.filter.get("view_kind").and_then(serde_json::Value::as_str)
                == Some("global-search")
        })
        .map(to_saved_global_search_view)
        .collect();
    let mut errors = Vec::new();
    let mut records = Vec::new();
    let mut sensors = Vec::new();
    let mut identities = Vec::new();
    let mut entities = Vec::new();
    let mut incidents = Vec::new();
    let mut actions = Vec::new();
    let mut events = Vec::new();
    let mut audits = Vec::new();

    if !term.is_empty() {
        let record_filter =
            RecordSearchFilter { query: Some(term.clone()), limit: 16, ..Default::default() };
        // "All sources" must include source records as well as the modeled/control-plane
        // categories below. The explicit scope still uses the same bounded query; this only
        // fixes the command surface's all-sources contract so a search for a raw ticket, email,
        // or payload value is not silently omitted from the default view.
        if scope_is(&scope, "records") {
            match state.stats_client.search_records(session.tenant_id, &record_filter).await {
                Ok(result) => {
                    records = result
                        .records
                        .into_iter()
                        .map(|record| RecordHit {
                            id: record.id,
                            title: record.preview(),
                            source_type: record.source_type,
                            connector_id: record.connector_id,
                            ingested_at: record.ingested_at.to_rfc3339(),
                        })
                        .collect();
                }
                Err(error) => errors.push(format!("records: {error}")),
            }
        }

        if scope_is(&scope, "sensors") {
            match state.sensors_client.list_sensors(session.tenant_id, 1000, 0).await {
                Ok(page) => {
                    sensors = page
                        .sensors
                        .into_iter()
                        .filter(|sensor| {
                            contains(&sensor.name, &term)
                                || contains(&sensor.connector_type, &term)
                                || contains(&sensor.id.to_string(), &term)
                        })
                        .map(|sensor| SensorHit {
                            id: sensor.id,
                            name: sensor.name,
                            connector_type: sensor.connector_type,
                            enabled: sensor.enabled,
                        })
                        .collect();
                }
                Err(error) => errors.push(format!("sensors: {error}")),
            }
        }

        if scope_is(&scope, "identities") {
            match state.users_client.list_users(session.tenant_id, session.role).await {
                Ok(users) => {
                    identities = users
                        .into_iter()
                        .filter(|user| {
                            contains(&user.username, &term)
                                || contains(&user.role.to_string(), &term)
                                || contains(&user.id.to_string(), &term)
                        })
                        .map(|user| IdentityHit {
                            id: user.id,
                            username: user.username,
                            role: user.role.to_string(),
                            mfa_enabled: user.mfa_enabled,
                        })
                        .collect();
                }
                Err(error) => errors.push(format!("identities: {error}")),
            }
        }

        if let Some(client) = ontology_client::global() {
            let (types, objects, action_types, invocations, links) = tokio::join!(
                async {
                    if scope_is(&scope, "entities") {
                        client.list_object_types(&session.bearer_token).await
                    } else {
                        Ok(Vec::new())
                    }
                },
                async {
                    if scope_is(&scope, "entities") || scope_is(&scope, "actions") {
                        client.list_objects(&session.bearer_token, None).await
                    } else {
                        Ok(Vec::new())
                    }
                },
                async {
                    if scope_is(&scope, "actions") {
                        client.list_action_types(&session.bearer_token).await
                    } else {
                        Ok(Vec::new())
                    }
                },
                async {
                    if scope_is(&scope, "actions") || scope_is(&scope, "entities") {
                        client.list_action_invocations(&session.bearer_token).await
                    } else {
                        Ok(Vec::new())
                    }
                },
                async {
                    if scope_is(&scope, "entities") {
                        client.list_links(&session.bearer_token).await
                    } else {
                        Ok(Vec::new())
                    }
                },
            );
            if scope_is(&scope, "entities") {
                let mut context_events = Vec::new();
                let mut context_incidents = Vec::new();
                match state
                    .events_client
                    .list_events(&session.bearer_token, 1000, 0, None, None)
                    .await
                {
                    Ok(page) => context_events = page.events,
                    Err(error) => errors.push(format!("entity signal context: {error}")),
                }
                match state.incidents_client.list_incidents(session.tenant_id, None).await {
                    Ok(items) => context_incidents = items,
                    Err(error) => errors.push(format!("entity case context: {error}")),
                }
                let mut events_by_record =
                    std::collections::HashMap::<uuid::Uuid, Vec<(uuid::Uuid, String)>>::new();
                for event in &context_events {
                    for record_id in &event.record_ids {
                        events_by_record
                            .entry(*record_id)
                            .or_default()
                            .push((event.id, event.event_type.clone()));
                    }
                }
                match (&types, &objects) {
                    (Ok(types), Ok(objects)) => {
                        let type_names = types
                            .into_iter()
                            .map(|item| (item.id, item.name.clone()))
                            .collect::<std::collections::HashMap<_, _>>();
                        for object in objects {
                            let title = object
                                .properties
                                .get("name")
                                .or_else(|| object.properties.get("title"))
                                .or_else(|| object.properties.get("subject"))
                                .or_else(|| object.properties.get("id"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Untitled entity")
                                .to_string();
                            let properties = object.properties.to_string();
                            if contains(&title, &term) || contains(&properties, &term) {
                                let state = object
                                    .properties
                                    .get("status")
                                    .or_else(|| object.properties.get("health"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("modeled")
                                    .to_string();
                                let related_count = links
                                    .as_ref()
                                    .map(|links| {
                                        links
                                            .iter()
                                            .filter(|link| {
                                                link.source_object_id == object.id
                                                    || link.target_object_id == object.id
                                            })
                                            .count()
                                    })
                                    .unwrap_or(0);
                                let action_count = invocations
                                    .as_ref()
                                    .map(|actions| {
                                        actions
                                            .iter()
                                            .filter(|action| {
                                                action
                                                    .target_object_ids
                                                    .as_array()
                                                    .into_iter()
                                                    .flatten()
                                                    .any(|target| {
                                                        target.as_str().and_then(|value| {
                                                            value.parse::<uuid::Uuid>().ok()
                                                        }) == Some(object.id)
                                                    })
                                            })
                                            .count()
                                    })
                                    .unwrap_or(0);
                                let lineage_ids = object
                                    .source_lineage
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|value| value.as_str())
                                    .filter_map(|value| value.parse::<uuid::Uuid>().ok())
                                    .collect::<Vec<_>>();
                                let mut signals = lineage_ids
                                    .iter()
                                    .filter_map(|record_id| events_by_record.get(record_id))
                                    .flatten()
                                    .map(|(id, event_type)| EntityContextLink {
                                        id: *id,
                                        label: event_type.clone(),
                                    })
                                    .collect::<Vec<_>>();
                                signals.sort_by_key(|link| link.id);
                                signals.dedup_by_key(|link| link.id);
                                let signal_count = signals.len();
                                let signal_ids = signals
                                    .iter()
                                    .map(|link| link.id)
                                    .collect::<std::collections::HashSet<_>>();
                                signals.truncate(4);
                                let mut all_cases = context_incidents
                                    .iter()
                                    .filter(|item| {
                                        item.event_ids.iter().any(|id| signal_ids.contains(id))
                                    })
                                    .map(|item| EntityContextLink {
                                        id: item.incident.id,
                                        label: item.incident.title.clone(),
                                    })
                                    .collect::<Vec<_>>();
                                let case_count = all_cases.len();
                                all_cases.sort_by_key(|link| link.id);
                                all_cases.dedup_by_key(|link| link.id);
                                let mut cases = all_cases;
                                cases.truncate(4);
                                entities.push(EntityHit {
                                    id: object.id,
                                    type_name: type_names
                                        .get(&object.object_type_id)
                                        .cloned()
                                        .unwrap_or_else(|| "Entity".to_string()),
                                    title,
                                    state,
                                    related_count,
                                    action_count,
                                    signal_count,
                                    case_count,
                                    signals,
                                    cases,
                                });
                            }
                        }
                    }
                    (Err(error), _) | (_, Err(error)) => errors.push(format!("ontology: {error}")),
                }
            }
            if scope_is(&scope, "actions") {
                let review_by_invocation = client
                    .list_action_reviews(&session.bearer_token)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|review| (review.invocation_id, review))
                    .collect::<std::collections::HashMap<_, _>>();
                match (action_types, invocations) {
                    (Ok(action_types), Ok(invocations)) => {
                        let action_names = action_types
                            .into_iter()
                            .map(|action| (action.id, action.name))
                            .collect::<std::collections::HashMap<_, _>>();
                        let object_titles = objects
                            .as_ref()
                            .map(|items| items.iter())
                            .into_iter()
                            .flatten()
                            .map(|object| {
                                let title = object
                                    .properties
                                    .get("name")
                                    .or_else(|| object.properties.get("title"))
                                    .or_else(|| object.properties.get("subject"))
                                    .or_else(|| object.properties.get("id"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("Untitled entity")
                                    .to_string();
                                (object.id, title)
                            })
                            .collect::<std::collections::HashMap<_, _>>();
                        for invocation in invocations {
                            let name = action_names
                                .get(&invocation.action_type_id)
                                .cloned()
                                .unwrap_or_else(|| "Unknown action".to_string());
                            let target_ids = invocation
                                .target_object_ids
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|value| value.as_str())
                                .filter_map(|value| value.parse::<uuid::Uuid>().ok())
                                .collect::<Vec<_>>();
                            let targets = target_ids
                                .iter()
                                .map(|id| ActionTargetHit {
                                    id: *id,
                                    label: object_titles
                                        .get(id)
                                        .cloned()
                                        .unwrap_or_else(|| id.to_string()),
                                })
                                .collect::<Vec<_>>();
                            let target_context = targets
                                .iter()
                                .map(|target| target.label.clone())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let searchable = format!(
                                "{} {} {} {}",
                                name,
                                invocation.outcome,
                                invocation.parameters,
                                invocation.triggering_event_ref
                            )
                            .to_ascii_lowercase();
                            if contains(&searchable, &term) || contains(&target_context, &term) {
                                let event_id = invocation
                                    .triggering_event_ref
                                    .get("event_id")
                                    .or_else(|| invocation.triggering_event_ref.get("id"))
                                    .and_then(serde_json::Value::as_str)
                                    .and_then(|value| value.parse::<uuid::Uuid>().ok());
                                let incident_id = invocation
                                    .triggering_event_ref
                                    .get("incident_id")
                                    .and_then(serde_json::Value::as_str)
                                    .and_then(|value| value.parse::<uuid::Uuid>().ok());
                                actions.push(ActionHit {
                                    id: invocation.id,
                                    name,
                                    outcome: invocation.outcome,
                                    review_status: review_by_invocation
                                        .get(&invocation.id)
                                        .map(|review| review.status.replace('_', " "))
                                        .unwrap_or_else(|| "not reviewed".to_string()),
                                    review_assignee: review_by_invocation
                                        .get(&invocation.id)
                                        .and_then(|review| review.assignee.clone()),
                                    review_stale: review_by_invocation
                                        .get(&invocation.id)
                                        .map(|review| {
                                            !matches!(
                                                review.status.as_str(),
                                                "approved" | "declined"
                                            ) && review
                                                .due_at
                                                .is_some_and(|due_at| due_at <= chrono::Utc::now())
                                        })
                                        .unwrap_or(false),
                                    target_context,
                                    targets,
                                    event_id,
                                    incident_id,
                                    executed_at: invocation.executed_at.to_rfc3339(),
                                });
                            }
                        }
                    }
                    (Err(error), _) | (_, Err(error)) => errors.push(format!("actions: {error}")),
                }
            }
        } else {
            if scope_is(&scope, "entities") || scope_is(&scope, "actions") {
                errors.push("ontology: client unavailable".to_string());
            }
        }

        if scope_is(&scope, "incidents") {
            let all_events = match state
                .events_client
                .list_events(&session.bearer_token, 1000, 0, None, None)
                .await
            {
                Ok(page) => page.events,
                Err(error) => {
                    errors.push(format!("incident signal context: {error}"));
                    Vec::new()
                }
            };
            let event_labels = all_events
                .into_iter()
                .map(|event| (event.id, event.event_type))
                .collect::<std::collections::HashMap<_, _>>();
            match state.incidents_client.list_incidents(session.tenant_id, None).await {
                Ok(items) => {
                    incidents = items
                        .into_iter()
                        .filter_map(|item| {
                            let event_count = item.event_ids.len();
                            let event_links = item
                                .event_ids
                                .iter()
                                .filter_map(|id| {
                                    event_labels.get(id).map(|event_type| EntityContextLink {
                                        id: *id,
                                        label: event_type.clone(),
                                    })
                                })
                                .collect::<Vec<_>>();
                            let searchable = format!(
                                "{} {} {}",
                                item.incident.title,
                                item.incident.summary,
                                event_links
                                    .iter()
                                    .map(|event| event.label.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            );
                            contains(&searchable, &term).then(|| IncidentHit {
                                id: item.incident.id,
                                title: item.incident.title,
                                severity: item.incident.severity.to_string(),
                                status: item.incident.status.to_string(),
                                event_count,
                                event_links,
                            })
                        })
                        .collect();
                }
                Err(error) => errors.push(format!("incidents: {error}")),
            }
        }

        if scope_is(&scope, "events") {
            match state.events_client.list_events(&session.bearer_token, 1000, 0, None, None).await
            {
                Ok(page) => {
                    events = page
                        .events
                        .into_iter()
                        .filter(|event| {
                            contains(&event.id.to_string(), &term)
                                || contains(&event.event_type, &term)
                                || contains(&event.group_key, &term)
                                || contains(&event.status, &term)
                                || event
                                    .record_ids
                                    .iter()
                                    .any(|id| contains(&id.to_string(), &term))
                        })
                        .take(24)
                        .map(|event| EventHit {
                            id: event.id,
                            event_type: event.event_type,
                            group_key: event.group_key,
                            status: event.status,
                            occurred_at: event.occurred_at.to_rfc3339(),
                            record_count: event.record_ids.len(),
                        })
                        .collect();
                }
                Err(error) => errors.push(format!("events: {error}")),
            }
        }

        if scope_is(&scope, "audit") {
            let (audit_entries, audit_errors) = crate::recent_audit_log_handler::fetch_merged_page(
                &state,
                session.tenant_id,
                &session.bearer_token,
                None,
                80,
            )
            .await;
            for error in audit_errors {
                errors.push(format!("audit: {error}"));
            }
            audits = audit_entries
                .into_iter()
                .filter_map(|(service, entry)| {
                    let searchable = format!(
                        "{} {} {} {} {} {}",
                        entry.id,
                        entry.entity_id,
                        entry.entity_type,
                        entry.change_type,
                        entry.actor,
                        entry.after,
                    );
                    contains(&searchable, &term).then(|| AuditHit {
                        id: entry.id,
                        entity_id: entry.entity_id,
                        service: service.to_string(),
                        entity_type: entry.entity_type,
                        change_type: entry.change_type,
                        actor: entry.actor,
                        changed_at: entry.changed_at.to_rfc3339(),
                    })
                })
                .take(24)
                .collect();
        }
    }

    Html(
        SearchTemplate {
            show_nav: true,
            is_admin,
            query: term,
            scope,
            saved_views,
            notice: query.notice,
            records,
            sensors,
            identities,
            entities,
            incidents,
            actions,
            events,
            audits,
            errors,
            searched: !query.q.trim().is_empty(),
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn governed_search_hits_expose_review_posture_and_exact_decision_links() {
        let template = include_str!("../templates/search.html");
        assert!(template.contains("/actions/{{ hit.id }}"));
        assert!(template.contains("review {{ hit.review_status }}"));
        assert!(template.contains("hit.review_stale"));
        assert!(template.contains("hit.review_assignee"));
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct SaveGlobalSearchViewForm {
    name: String,
    q: String,
    scope: String,
}

pub async fn post_save_global_search_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<SaveGlobalSearchViewForm>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let scope = normalize_scope(&form.scope);
    let filter = serde_json::json!({
        "view_kind": "global-search",
        "q": form.q,
        "scope": scope,
    });
    match state.saved_search_queries_client.create(session.tenant_id, &form.name, filter).await {
        Ok(_) => {
            let params = serde_urlencoded::to_string([
                ("q", form.q),
                ("scope", scope.to_string()),
                ("notice", "saved".to_string()),
            ])
            .unwrap_or_default();
            axum::response::Redirect::to(&format!("/search?{params}")).into_response()
        }
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

pub async fn post_delete_global_search_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    match state.saved_search_queries_client.delete(session.tenant_id, id).await {
        Ok(()) => axum::response::Redirect::to("/search?notice=deleted").into_response(),
        Err(error) => (axum::http::StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod search_scope_tests {
    use super::{normalize_scope, scope_is};

    #[test]
    fn normalizes_supported_scopes_and_defaults_unknown_values() {
        assert_eq!(normalize_scope(" ENTITIES "), "entities");
        assert_eq!(normalize_scope("connectors"), "all");
        assert_eq!(normalize_scope("sensors"), "sensors");
        assert_eq!(normalize_scope("identities"), "identities");
        assert_eq!(normalize_scope("audit"), "audit");
        assert_eq!(normalize_scope("not-a-scope"), "all");
    }

    #[test]
    fn all_scope_includes_every_category() {
        assert!(scope_is("all", "records"));
        assert!(scope_is("all", "audit"));
        assert!(!scope_is("events", "entities"));
    }

    #[test]
    fn governed_search_results_expose_direct_ontology_handoffs() {
        let template = include_str!("../templates/search.html");
        assert!(template.contains("Targets:"));
        assert!(template.contains("/ontology?object_id={{ target.id }}#object-{{ target.id }}"));
        assert!(template.contains("<div class=\"search-hit\">"));
        assert!(template.contains("{{ hit.related_count }} relationship"));
        assert!(template.contains("{{ hit.action_count }} governed action"));
        assert!(template.contains("{{ hit.signal_count }} linked signal"));
        assert!(template.contains("{{ hit.case_count }} linked case"));
        assert!(template.contains("/events/{{ signal.id }}"));
        assert!(template.contains("/incidents/{{ case.id }}"));
        assert!(template.contains("{{ hit.event_count }} linked signal"));
        assert!(template.contains("hit.event_links"));
        assert!(template.contains("search-operating-chain"));
        assert!(template.contains("scope=entities"));
        assert!(template.contains("Governed actions"));
    }
}
