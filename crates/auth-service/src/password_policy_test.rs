use super::*;

#[test]
fn accepts_a_reasonably_long_unique_password() {
    assert_eq!(validate_password_strength("correct-horse-battery-staple-2026", "alice"), Ok(()));
}

#[test]
fn rejects_a_password_shorter_than_the_minimum() {
    assert_eq!(validate_password_strength("short1", "alice"), Err(PasswordPolicyError::TooShort));
}

#[test]
fn rejects_a_password_longer_than_the_maximum() {
    let too_long = "a".repeat(200);
    assert_eq!(validate_password_strength(&too_long, "alice"), Err(PasswordPolicyError::TooLong));
}

#[test]
fn rejects_a_password_equal_to_the_username_case_insensitively() {
    assert_eq!(
        validate_password_strength("ALICE-ALICE-ALICE-ALICE", "alice-alice-alice-alice"),
        Err(PasswordPolicyError::MatchesUsername)
    );
}

#[test]
fn rejects_a_common_password() {
    assert_eq!(
        validate_password_strength("correcthorsebatterystaple", "alice"),
        Err(PasswordPolicyError::TooCommon)
    );
}

#[test]
fn rejects_a_common_password_regardless_of_case() {
    assert_eq!(
        validate_password_strength("CorrectHorseBatteryStaple", "alice"),
        Err(PasswordPolicyError::TooCommon)
    );
}

#[test]
fn a_password_at_exactly_the_minimum_length_is_accepted() {
    let exactly_min = "a".repeat(12);
    assert_eq!(validate_password_strength(&exactly_min, "alice"), Ok(()));
}

#[test]
fn a_password_at_exactly_the_maximum_length_is_accepted() {
    let exactly_max: String = "x".repeat(MAX_LENGTH);
    assert_eq!(validate_password_strength(&exactly_max, "alice"), Ok(()));
}

#[test]
fn a_password_one_over_the_maximum_length_is_rejected() {
    let one_over: String = "x".repeat(MAX_LENGTH + 1);
    assert_eq!(validate_password_strength(&one_over, "alice"), Err(PasswordPolicyError::TooLong));
}

#[test]
fn summary_reflects_the_actual_enforced_constants() {
    let s = summary();
    assert_eq!(s.min_length, MIN_LENGTH);
    assert_eq!(s.max_length, MAX_LENGTH);
    assert_eq!(s.blocklist_size, COMMON_PASSWORDS.len());
}
