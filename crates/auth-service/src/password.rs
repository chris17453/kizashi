#[path = "password_test.rs"]
#[cfg(test)]
mod password_test;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("failed to hash password: {0}")]
    Hash(String),
}

/// Hashes a plaintext password with Argon2id (spec §8: "local login... hashed credentials —
/// bcrypt/argon2"). The salt is generated fresh per call and embedded in the returned PHC
/// string, so two hashes of the same password never match byte-for-byte.
pub fn hash_password(plaintext: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| PasswordError::Hash(e.to_string()))
}

/// Verifies a plaintext password against a stored PHC hash string. Returns `false` (not an
/// error) for a malformed stored hash or a mismatched password — callers can't distinguish
/// "wrong password" from "corrupted hash" and shouldn't need to, since both mean "deny login."
pub fn verify_password(plaintext: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default().verify_password(plaintext.as_bytes(), &parsed_hash).is_ok()
}
