use super::*;
use crate::{Dek, SubjectRef};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn subject(id: &str) -> SubjectRef {
    SubjectRef::new(Uuid::nil(), "contact", id)
}

#[test]
fn get_returns_inserted_dek_within_ttl() {
    let cache = DekCache::new(Duration::from_secs(60));
    let dek = Arc::new(Dek::from_bytes([1; 32]));
    cache.insert(subject("a"), dek.clone());
    assert_eq!(cache.get(&subject("a")).unwrap().as_bytes(), dek.as_bytes());
    assert!(cache.get(&subject("b")).is_none());
}

#[test]
fn entries_expire_after_ttl() {
    let cache = DekCache::new(Duration::from_millis(20));
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    std::thread::sleep(Duration::from_millis(40));
    assert!(cache.get(&subject("a")).is_none());
    assert_eq!(cache.len(), 0, "expired entry is dropped on access");
}

#[test]
fn zero_ttl_never_serves_from_cache() {
    let cache = DekCache::new(Duration::ZERO);
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    assert!(cache.get(&subject("a")).is_none());
}

#[test]
fn evict_removes_only_that_subject() {
    let cache = DekCache::new(Duration::from_secs(60));
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    cache.insert(subject("b"), Arc::new(Dek::generate()));
    cache.evict(&subject("a"));
    assert!(cache.get(&subject("a")).is_none());
    assert!(cache.get(&subject("b")).is_some());
}

#[test]
fn evict_tenant_removes_every_subject_of_that_tenant_only() {
    let cache = DekCache::new(Duration::from_secs(60));
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    cache.insert(SubjectRef::new(t1, "contact", "a"), Arc::new(Dek::generate()));
    cache.insert(SubjectRef::new(t1, "mailbox", "b"), Arc::new(Dek::generate()));
    cache.insert(SubjectRef::new(t2, "contact", "a"), Arc::new(Dek::generate()));
    cache.evict_tenant(t1);
    assert_eq!(cache.len(), 1);
    assert!(cache.get(&SubjectRef::new(t2, "contact", "a")).is_some());
}

#[test]
fn capacity_bound_evicts_oldest_entry() {
    let cache = DekCache::with_capacity(Duration::from_secs(60), 2);
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    std::thread::sleep(Duration::from_millis(2));
    cache.insert(subject("b"), Arc::new(Dek::generate()));
    std::thread::sleep(Duration::from_millis(2));
    cache.insert(subject("c"), Arc::new(Dek::generate()));
    assert_eq!(cache.len(), 2);
    assert!(cache.get(&subject("a")).is_none());
    assert!(cache.get(&subject("b")).is_some());
    assert!(cache.get(&subject("c")).is_some());
}

#[test]
fn reinserting_existing_subject_at_capacity_does_not_evict_others() {
    let cache = DekCache::with_capacity(Duration::from_secs(60), 2);
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    cache.insert(subject("b"), Arc::new(Dek::generate()));
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    assert_eq!(cache.len(), 2);
    assert!(cache.get(&subject("b")).is_some());
}

#[test]
fn clear_empties_the_cache() {
    let cache = DekCache::new(Duration::from_secs(60));
    cache.insert(subject("a"), Arc::new(Dek::generate()));
    cache.clear();
    assert!(cache.is_empty());
}
