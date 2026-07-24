use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryComplianceHoldRepository {
    pub holds: Mutex<Vec<ComplianceHold>>,
}

#[async_trait]
impl ComplianceHoldRepository for InMemoryComplianceHoldRepository {
    async fn create(
        &self,
        hold: ComplianceHold,
        _actor: &str,
    ) -> Result<ComplianceHold, ComplianceHoldRepositoryError> {
        self.holds.lock().unwrap().push(hold.clone());
        Ok(hold)
    }
    async fn list(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ComplianceHold>, ComplianceHoldRepositoryError> {
        Ok(self
            .holds
            .lock()
            .unwrap()
            .iter()
            .filter(|hold| hold.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
    async fn has_active(
        &self,
        tenant_id: Uuid,
        data_class: DataClass,
    ) -> Result<bool, ComplianceHoldRepositoryError> {
        Ok(self.holds.lock().unwrap().iter().any(|hold| {
            hold.tenant_id == tenant_id && hold.data_class == data_class && hold.active
        }))
    }
    async fn release(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        _actor: &str,
    ) -> Result<ComplianceHold, ComplianceHoldRepositoryError> {
        let mut holds = self.holds.lock().unwrap();
        let hold = holds
            .iter_mut()
            .find(|hold| hold.tenant_id == tenant_id && hold.id == id)
            .ok_or(ComplianceHoldRepositoryError::NotFound(id))?;
        hold.active = false;
        Ok(hold.clone())
    }
}

#[test]
fn hold_defaults_are_active_and_a_release_is_distinct() {
    let hold = ComplianceHold {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        data_class: DataClass::Raw,
        reason: "legal review".into(),
        active: true,
        created_by: "operator".into(),
        created_at: Utc::now(),
        released_at: None,
    };
    let released = ComplianceHold { active: false, released_at: Some(Utc::now()), ..hold.clone() };
    assert!(hold.active);
    assert!(!released.active);
    assert!(released.released_at.is_some());
}
