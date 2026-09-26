//! `subject.shredded` bus message: emitted after a subject's DEK is destroyed so every node
//! evicts its cached DEK and downstream stores can hard-delete what is cheap to delete.

#[path = "subject_shredded_test.rs"]
#[cfg(test)]
mod subject_shredded_test;

use crate::SubjectRef;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SUBJECT_SHREDDED_EXCHANGE: &str = "subject.shredded";
pub const SUBJECT_SHREDDED_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectShredded {
    pub id: Uuid,
    pub schema_version: u16,
    pub tenant_id: Uuid,
    pub subject_type: String,
    pub subject_id: String,
    pub shredded_at: DateTime<Utc>,
    pub actor: String,
}

impl SubjectShredded {
    pub fn new(subject: &SubjectRef, shredded_at: DateTime<Utc>, actor: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            schema_version: SUBJECT_SHREDDED_SCHEMA_VERSION,
            tenant_id: subject.tenant_id,
            subject_type: subject.subject_type.clone(),
            subject_id: subject.subject_id.clone(),
            shredded_at,
            actor: actor.to_string(),
        }
    }

    pub fn subject(&self) -> SubjectRef {
        SubjectRef::new(self.tenant_id, self.subject_type.clone(), self.subject_id.clone())
    }
}
