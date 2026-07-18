//! Confirms the `hash_password` bootstrap CLI (used by `scripts/seed-local-demo.sh`) produces a
//! hash `verify_password` actually accepts — not just "the binary runs," but that its output is
//! usable for what it exists for.

use auth_service::verify_password;
use std::process::Command;

#[test]
fn produces_a_hash_that_verify_password_accepts() {
    let output = Command::new(env!("CARGO_BIN_EXE_hash_password"))
        .arg("correct-horse-battery-staple")
        .output()
        .expect("failed to run hash_password binary");

    assert!(output.status.success());
    let hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
    assert!(hash.starts_with("$argon2id$"));
    assert!(verify_password("correct-horse-battery-staple", &hash));
    assert!(!verify_password("wrong-password", &hash));
}

#[test]
fn exits_nonzero_with_no_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_hash_password"))
        .output()
        .expect("failed to run hash_password binary");
    assert!(!output.status.success());
}
