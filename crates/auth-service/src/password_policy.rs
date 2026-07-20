#[path = "password_policy_test.rs"]
#[cfg(test)]
mod password_policy_test;

use thiserror::Error;

/// NIST 800-63B's modern guidance deprioritizes composition rules (must contain a digit/symbol)
/// in favor of length plus a blocklist of known-weak values, which is what this enforces —
/// composition rules push users toward predictable substitutions ("Password1!") that don't
/// actually resist guessing, while length and a blocklist do.
const MIN_LENGTH: usize = 12;
/// Argon2's hashing cost scales with input size; an unbounded password is a cheap way to make a
/// single login/create-user request expensive to hash, a minor but free-to-close DoS surface.
const MAX_LENGTH: usize = 128;

/// Not an exhaustive breached-password list (that's a much larger, externally-sourced dataset
/// out of scope for v1) -- just the handful of values so common that allowing them at all would
/// be a compliance-review red flag on its own.
const COMMON_PASSWORDS: &[&str] = &[
    "password",
    "password123",
    "123456789012",
    "qwertyuiop12",
    "letmein12345",
    "admin1234567",
    "welcome12345",
    "changeme1234",
    "iloveyou1234",
    "correcthorsebatterystaple",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PasswordPolicyError {
    #[error("password must be at least {MIN_LENGTH} characters")]
    TooShort,
    #[error("password must be at most {MAX_LENGTH} characters")]
    TooLong,
    #[error("password is too common to be secure")]
    TooCommon,
    #[error("password must not be the same as the username")]
    MatchesUsername,
}

/// Enforced once, at the only place a password is ever set today (`create_user` -- there is no
/// self-service password-change endpoint yet, tracked as a follow-up). Case-insensitive
/// comparisons throughout, since "Password123456" is exactly as guessable as "password123456".
pub fn validate_password_strength(
    password: &str,
    username: &str,
) -> Result<(), PasswordPolicyError> {
    if password.chars().count() < MIN_LENGTH {
        return Err(PasswordPolicyError::TooShort);
    }
    if password.chars().count() > MAX_LENGTH {
        return Err(PasswordPolicyError::TooLong);
    }
    let lower = password.to_lowercase();
    if lower.eq_ignore_ascii_case(username) {
        return Err(PasswordPolicyError::MatchesUsername);
    }
    if COMMON_PASSWORDS.iter().any(|common| lower == *common) {
        return Err(PasswordPolicyError::TooCommon);
    }
    Ok(())
}
