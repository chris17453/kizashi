#[path = "incident_correlation_model_test.rs"]
#[cfg(test)]
mod incident_correlation_model_test;

use crate::{EventSummary, IncidentDetail};
use common::IncidentStatus;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct CorrelationCandidate {
    pub event_id: Uuid,
    pub incident_id: Uuid,
    pub incident_title: String,
    pub group_key: String,
    pub event_type: String,
    pub occurred_at: String,
    pub confidence: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct CorrelationIncidentOption {
    pub id: Uuid,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct AmbiguousCorrelationGroup {
    pub group_key: String,
    pub event_ids: Vec<Uuid>,
    pub incident_options: Vec<CorrelationIncidentOption>,
}

/// Finds unlinked events whose group key maps to exactly one active incident. Ambiguous keys
/// are deliberately excluded: a sweep must never silently choose between two investigations.
pub(crate) fn build_correlation_candidates(
    incidents: &[IncidentDetail],
    events: &[EventSummary],
) -> Vec<CorrelationCandidate> {
    let event_by_id = events.iter().map(|event| (event.id, event)).collect::<HashMap<_, _>>();
    let linked = incidents
        .iter()
        .flat_map(|incident| incident.event_ids.iter().copied())
        .collect::<HashSet<_>>();
    let mut incident_ids_by_key = HashMap::<String, HashSet<Uuid>>::new();
    for incident in
        incidents.iter().filter(|incident| incident.incident.status != IncidentStatus::Resolved)
    {
        for event_id in &incident.event_ids {
            let Some(event) = event_by_id.get(event_id) else { continue };
            let key = event.group_key.trim().to_ascii_lowercase();
            if !key.is_empty() {
                incident_ids_by_key.entry(key).or_default().insert(incident.incident.id);
            }
        }
    }
    let incident_by_id = incidents
        .iter()
        .map(|incident| (incident.incident.id, incident))
        .collect::<HashMap<_, _>>();
    let mut candidates = events
        .iter()
        .filter(|event| !linked.contains(&event.id))
        .filter_map(|event| {
            let key = event.group_key.trim().to_ascii_lowercase();
            let incident_ids = incident_ids_by_key.get(&key)?;
            if incident_ids.len() != 1 {
                return None;
            }
            let incident_id = *incident_ids.iter().next()?;
            let incident = incident_by_id.get(&incident_id)?;
            Some(CorrelationCandidate {
                event_id: event.id,
                incident_id,
                incident_title: incident.incident.title.clone(),
                group_key: event.group_key.clone(),
                event_type: event.event_type.clone(),
                occurred_at: event.occurred_at.to_rfc3339(),
                confidence: "high".to_string(),
                evidence: vec![
                    "normalized group key matches exactly".to_string(),
                    "exactly one active target case".to_string(),
                ],
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.incident_title
            .cmp(&right.incident_title)
            .then_with(|| left.group_key.cmp(&right.group_key))
            .then_with(|| left.occurred_at.cmp(&right.occurred_at))
    });
    candidates
}

/// Groups unlinked events whose key belongs to multiple active incidents. These groups are
/// reviewable, but never produce an implicit target: an operator must explicitly choose one of
/// the active incident options before any link is written.
pub(crate) fn build_ambiguous_correlation_groups(
    incidents: &[IncidentDetail],
    events: &[EventSummary],
) -> Vec<AmbiguousCorrelationGroup> {
    let event_by_id = events.iter().map(|event| (event.id, event)).collect::<HashMap<_, _>>();
    let linked = incidents
        .iter()
        .flat_map(|incident| incident.event_ids.iter().copied())
        .collect::<HashSet<_>>();
    let active_incidents = incidents
        .iter()
        .filter(|incident| incident.incident.status != IncidentStatus::Resolved)
        .collect::<Vec<_>>();
    let mut incident_ids_by_key = HashMap::<String, HashSet<Uuid>>::new();
    for incident in &active_incidents {
        for event_id in &incident.event_ids {
            let Some(event) = event_by_id.get(event_id) else { continue };
            let key = event.group_key.trim().to_ascii_lowercase();
            if !key.is_empty() {
                incident_ids_by_key.entry(key).or_default().insert(incident.incident.id);
            }
        }
    }
    let mut event_ids_by_key = HashMap::<String, Vec<Uuid>>::new();
    for event in events.iter().filter(|event| !linked.contains(&event.id)) {
        let key = event.group_key.trim().to_ascii_lowercase();
        if incident_ids_by_key.get(&key).is_some_and(|ids| ids.len() > 1) {
            event_ids_by_key.entry(key).or_default().push(event.id);
        }
    }
    let incident_by_id = active_incidents
        .into_iter()
        .map(|incident| (incident.incident.id, incident))
        .collect::<HashMap<_, _>>();
    let mut groups = event_ids_by_key
        .into_iter()
        .filter_map(|(group_key, mut event_ids)| {
            let mut incident_options = incident_ids_by_key
                .get(&group_key)?
                .iter()
                .filter_map(|id| incident_by_id.get(id))
                .map(|incident| CorrelationIncidentOption {
                    id: incident.incident.id,
                    title: incident.incident.title.clone(),
                })
                .collect::<Vec<_>>();
            event_ids.sort();
            incident_options
                .sort_by(|left, right| left.title.cmp(&right.title).then(left.id.cmp(&right.id)));
            Some(AmbiguousCorrelationGroup { group_key, event_ids, incident_options })
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| left.group_key.cmp(&right.group_key));
    groups
}
