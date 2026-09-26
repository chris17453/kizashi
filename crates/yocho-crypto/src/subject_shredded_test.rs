use super::*;
use crate::SubjectRef;
use uuid::Uuid;

#[test]
fn new_copies_the_subject_and_assigns_an_id() {
    let s = SubjectRef::new(Uuid::new_v4(), "contact", "c-1");
    let at = chrono::Utc::now();
    let a = SubjectShredded::new(&s, at, "user:ops");
    let b = SubjectShredded::new(&s, at, "user:ops");
    assert_ne!(a.id, b.id);
    assert_eq!(a.subject(), s);
    assert_eq!(a.shredded_at, at);
    assert_eq!(a.actor, "user:ops");
    assert_eq!(a.schema_version, SUBJECT_SHREDDED_SCHEMA_VERSION);
}

#[test]
fn exchange_name_matches_bus_convention() {
    assert_eq!(SUBJECT_SHREDDED_EXCHANGE, "subject.shredded");
}
