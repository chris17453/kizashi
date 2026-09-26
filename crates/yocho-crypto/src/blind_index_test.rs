use super::*;
use crate::IndexKey;

#[test]
fn normalize_email_trims_and_lowercases() {
    assert_eq!(normalize_email("  Alice.Smith@Example.COM \n"), "alice.smith@example.com");
}

#[test]
fn index_is_deterministic_for_same_key_and_input() {
    let indexer = BlindIndexer::new(IndexKey::from_bytes([1; 32]));
    assert_eq!(indexer.index("email", "a@b.c"), indexer.index("email", "a@b.c"));
}

#[test]
fn email_index_is_normalisation_insensitive() {
    let indexer = BlindIndexer::new(IndexKey::from_bytes([1; 32]));
    assert_eq!(indexer.index_email("Bob@Example.com "), indexer.index_email("bob@example.com"));
}

#[test]
fn different_tenant_keys_give_different_indexes_for_same_input() {
    let a = BlindIndexer::new(IndexKey::from_bytes([1; 32]));
    let b = BlindIndexer::new(IndexKey::from_bytes([2; 32]));
    assert_ne!(a.index_email("bob@example.com"), b.index_email("bob@example.com"));
}

#[test]
fn field_name_is_domain_separated() {
    let indexer = BlindIndexer::new(IndexKey::from_bytes([1; 32]));
    assert_ne!(indexer.index("email", "x"), indexer.index("subject", "x"));
    assert_ne!(indexer.index("ab", "c"), indexer.index("a", "bc"));
}

#[test]
fn hex_rendering_is_64_lowercase_chars() {
    let indexer = BlindIndexer::new(IndexKey::from_bytes([1; 32]));
    let hex = indexer.index("email", "x").to_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}
