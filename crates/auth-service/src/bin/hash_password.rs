//! Offline Argon2id password-hash generator (spec §8: "local login... hashed credentials").
//! Every real deployment needs some way to seed its first local user before any admin UI can
//! create one through the API — this is that bootstrap path, reusing `hash_password` (already
//! unit-tested in `password_test.rs`) rather than duplicating the hashing logic.
//!
//! Usage: cargo run -p auth-service --bin hash_password -- <plaintext-password>

fn main() {
    let plaintext = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: hash_password <plaintext-password>");
        std::process::exit(1);
    });
    let hash = auth_service::hash_password(&plaintext).expect("failed to hash password");
    println!("{hash}");
}
