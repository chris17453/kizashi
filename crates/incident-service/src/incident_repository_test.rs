use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryIncidentRepository {
    pub incidents: Mutex<Vec<Incident>>,
    pub links: Mutex<Vec<(Uuid, Uuid)>>,
    pub notes: Mutex<Vec<IncidentNote>>,
}

impl InMemoryIncidentRepository {
    pub fn with_incident(incident: Incident) -> Self {
        Self {
            incidents: Mutex::new(vec![incident]),
            links: Mutex::new(vec![]),
            notes: Mutex::new(vec![]),
        }
    }
}

#[async_trait]
impl IncidentRepository for InMemoryIncidentRepository {
    async fn create(
        &self,
        incident: Incident,
        initial_event_ids: &[Uuid],
        _actor: &str,
    ) -> Result<Incident, IncidentRepositoryError> {
        self.incidents.lock().unwrap().push(incident.clone());
        let mut links = self.links.lock().unwrap();
        for event_id in initial_event_ids {
            links.push((incident.id, *event_id));
        }
        Ok(incident)
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Incident>, IncidentRepositoryError> {
        Ok(self
            .incidents
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id && i.tenant_id == tenant_id)
            .cloned())
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<Incident>, IncidentRepositoryError> {
        Ok(self
            .incidents
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.tenant_id == tenant_id)
            .filter(|i| status_filter.map(|s| i.status == s).unwrap_or(true))
            .cloned()
            .collect())
    }

    async fn update(
        &self,
        incident: Incident,
        _actor: &str,
    ) -> Result<Incident, IncidentRepositoryError> {
        let mut incidents = self.incidents.lock().unwrap();
        match incidents
            .iter_mut()
            .find(|i| i.id == incident.id && i.tenant_id == incident.tenant_id)
        {
            Some(existing) => {
                *existing = incident.clone();
                Ok(incident)
            }
            None => Err(IncidentRepositoryError::NotFound(incident.id)),
        }
    }

    async fn link_event(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
        _actor: &str,
    ) -> Result<(), IncidentRepositoryError> {
        let exists = self
            .incidents
            .lock()
            .unwrap()
            .iter()
            .any(|i| i.id == incident_id && i.tenant_id == tenant_id);
        if !exists {
            return Err(IncidentRepositoryError::NotFound(incident_id));
        }
        self.links.lock().unwrap().push((incident_id, event_id));
        Ok(())
    }

    async fn unlink_event(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        event_id: Uuid,
        _actor: &str,
    ) -> Result<(), IncidentRepositoryError> {
        let exists = self
            .incidents
            .lock()
            .unwrap()
            .iter()
            .any(|i| i.id == incident_id && i.tenant_id == tenant_id);
        if !exists {
            return Err(IncidentRepositoryError::NotFound(incident_id));
        }
        self.links.lock().unwrap().retain(|(i, e)| !(*i == incident_id && *e == event_id));
        Ok(())
    }

    async fn list_linked_event_ids(
        &self,
        incident_id: Uuid,
    ) -> Result<Vec<Uuid>, IncidentRepositoryError> {
        Ok(self
            .links
            .lock()
            .unwrap()
            .iter()
            .filter(|(i, _)| *i == incident_id)
            .map(|(_, e)| *e)
            .collect())
    }

    async fn list_notes(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
    ) -> Result<Vec<IncidentNote>, IncidentRepositoryError> {
        Ok(self
            .notes
            .lock()
            .unwrap()
            .iter()
            .filter(|note| note.tenant_id == tenant_id && note.incident_id == incident_id)
            .cloned()
            .collect())
    }

    async fn add_note(
        &self,
        tenant_id: Uuid,
        incident_id: Uuid,
        author: &str,
        body: &str,
    ) -> Result<IncidentNote, IncidentRepositoryError> {
        if !self
            .incidents
            .lock()
            .unwrap()
            .iter()
            .any(|incident| incident.tenant_id == tenant_id && incident.id == incident_id)
        {
            return Err(IncidentRepositoryError::NotFound(incident_id));
        }
        let note = IncidentNote {
            id: Uuid::new_v4(),
            tenant_id,
            incident_id,
            author: author.to_string(),
            body: body.to_string(),
            created_at: chrono::Utc::now(),
        };
        self.notes.lock().unwrap().push(note.clone());
        Ok(note)
    }
}

pub struct FailingIncidentRepository;

#[async_trait]
impl IncidentRepository for FailingIncidentRepository {
    async fn create(
        &self,
        _incident: Incident,
        _initial_event_ids: &[Uuid],
        _actor: &str,
    ) -> Result<Incident, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Incident>, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _status_filter: Option<IncidentStatus>,
    ) -> Result<Vec<Incident>, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update(
        &self,
        _incident: Incident,
        _actor: &str,
    ) -> Result<Incident, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn link_event(
        &self,
        _tenant_id: Uuid,
        _incident_id: Uuid,
        _event_id: Uuid,
        _actor: &str,
    ) -> Result<(), IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn unlink_event(
        &self,
        _tenant_id: Uuid,
        _incident_id: Uuid,
        _event_id: Uuid,
        _actor: &str,
    ) -> Result<(), IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_linked_event_ids(
        &self,
        _incident_id: Uuid,
    ) -> Result<Vec<Uuid>, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_notes(
        &self,
        _tenant_id: Uuid,
        _incident_id: Uuid,
    ) -> Result<Vec<IncidentNote>, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }

    async fn add_note(
        &self,
        _tenant_id: Uuid,
        _incident_id: Uuid,
        _author: &str,
        _body: &str,
    ) -> Result<IncidentNote, IncidentRepositoryError> {
        Err(IncidentRepositoryError::Backend("simulated failure".to_string()))
    }
}

fn sample_incident(tenant_id: Uuid) -> Incident {
    Incident {
        id: Uuid::new_v4(),
        tenant_id,
        title: "elevated error rate".to_string(),
        summary: String::new(),
        severity: IncidentSeverity::High,
        status: IncidentStatus::Open,
        assigned_to: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        resolved_at: None,
    }
}

#[tokio::test]
async fn create_then_get_round_trips() {
    let repo = InMemoryIncidentRepository::default();
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);

    repo.create(incident.clone(), &[], "test-actor").await.unwrap();
    let found = repo.get(tenant_id, incident.id).await.unwrap();
    assert_eq!(found, Some(incident));
}

#[tokio::test]
async fn create_links_initial_events() {
    let repo = InMemoryIncidentRepository::default();
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let event_id = Uuid::new_v4();

    repo.create(incident.clone(), &[event_id], "test-actor").await.unwrap();

    let linked = repo.list_linked_event_ids(incident.id).await.unwrap();
    assert_eq!(linked, vec![event_id]);
}

#[tokio::test]
async fn update_of_unknown_incident_returns_not_found() {
    let repo = InMemoryIncidentRepository::default();
    let incident = sample_incident(Uuid::new_v4());

    let err = repo.update(incident, "test-actor").await.unwrap_err();
    assert!(matches!(err, IncidentRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn list_is_scoped_to_tenant() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryIncidentRepository::with_incident(sample_incident(tenant_id));
    repo.create(sample_incident(Uuid::new_v4()), &[], "test-actor").await.unwrap();

    let found = repo.list(tenant_id, None).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn list_filters_by_status() {
    let tenant_id = Uuid::new_v4();
    let repo = InMemoryIncidentRepository::default();
    let mut open = sample_incident(tenant_id);
    open.status = IncidentStatus::Open;
    let mut resolved = sample_incident(tenant_id);
    resolved.status = IncidentStatus::Resolved;
    repo.create(open.clone(), &[], "test-actor").await.unwrap();
    repo.create(resolved, &[], "test-actor").await.unwrap();

    let found = repo.list(tenant_id, Some(IncidentStatus::Open)).await.unwrap();
    assert_eq!(found, vec![open]);
}

#[tokio::test]
async fn link_event_then_unlink_removes_it() {
    let tenant_id = Uuid::new_v4();
    let incident = sample_incident(tenant_id);
    let repo = InMemoryIncidentRepository::with_incident(incident.clone());
    let event_id = Uuid::new_v4();

    repo.link_event(tenant_id, incident.id, event_id, "test-actor").await.unwrap();
    assert_eq!(repo.list_linked_event_ids(incident.id).await.unwrap(), vec![event_id]);

    repo.unlink_event(tenant_id, incident.id, event_id, "test-actor").await.unwrap();
    assert!(repo.list_linked_event_ids(incident.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn link_event_on_unknown_incident_returns_not_found() {
    let repo = InMemoryIncidentRepository::default();
    let err = repo
        .link_event(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), "test-actor")
        .await
        .unwrap_err();
    assert!(matches!(err, IncidentRepositoryError::NotFound(_)));
}

#[tokio::test]
async fn link_event_rejects_a_tenant_that_does_not_own_the_incident() {
    let incident = sample_incident(Uuid::new_v4());
    let repo = InMemoryIncidentRepository::with_incident(incident.clone());

    let err = repo
        .link_event(Uuid::new_v4(), incident.id, Uuid::new_v4(), "test-actor")
        .await
        .unwrap_err();
    assert!(matches!(err, IncidentRepositoryError::NotFound(_)));
}
