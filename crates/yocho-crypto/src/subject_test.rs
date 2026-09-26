use super::*;
use uuid::Uuid;

#[test]
fn aad_differs_when_tenant_differs() {
    let a = SubjectRef::new(Uuid::new_v4(), "contact", "c-1");
    let b = SubjectRef::new(Uuid::new_v4(), "contact", "c-1");
    assert_ne!(a.aad(), b.aad());
}

#[test]
fn aad_differs_when_subject_type_or_id_differs() {
    let tenant = Uuid::new_v4();
    let base = SubjectRef::new(tenant, "contact", "c-1");
    assert_ne!(base.aad(), SubjectRef::new(tenant, "mailbox", "c-1").aad());
    assert_ne!(base.aad(), SubjectRef::new(tenant, "contact", "c-2").aad());
}

#[test]
fn aad_is_length_prefixed_so_boundaries_cannot_be_shifted() {
    let tenant = Uuid::new_v4();
    let a = SubjectRef::new(tenant, "ab", "c");
    let b = SubjectRef::new(tenant, "a", "bc");
    assert_ne!(a.aad(), b.aad());
}

#[test]
fn aad_is_deterministic() {
    let s = SubjectRef::new(Uuid::new_v4(), "account", "acct-9");
    assert_eq!(s.aad(), s.clone().aad());
}
