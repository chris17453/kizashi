use super::*;

#[test]
fn retry_budget_dead_letters_at_the_bound() {
    assert!(!should_dead_letter(MAX_OUTBOX_PUBLISH_ATTEMPTS - 1));
    assert!(should_dead_letter(MAX_OUTBOX_PUBLISH_ATTEMPTS));
}
