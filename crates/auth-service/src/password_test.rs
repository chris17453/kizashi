use super::*;

#[test]
fn hash_password_produces_a_verifiable_hash() {
    let hash = hash_password("correct-horse-battery-staple").unwrap();
    assert!(verify_password("correct-horse-battery-staple", &hash));
}

#[test]
fn verify_password_rejects_the_wrong_password() {
    let hash = hash_password("correct-horse-battery-staple").unwrap();
    assert!(!verify_password("wrong-password", &hash));
}

#[test]
fn hash_password_salts_so_identical_passwords_hash_differently() {
    let hash1 = hash_password("same-password").unwrap();
    let hash2 = hash_password("same-password").unwrap();
    assert_ne!(hash1, hash2);
    assert!(verify_password("same-password", &hash1));
    assert!(verify_password("same-password", &hash2));
}

#[test]
fn verify_password_rejects_a_malformed_hash_without_panicking() {
    assert!(!verify_password("anything", "not-a-valid-phc-hash"));
}
