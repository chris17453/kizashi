use super::*;
use crate::{CryptoError, Dek, SubjectRef};
use proptest::prelude::*;
use uuid::Uuid;

fn subject() -> SubjectRef {
    SubjectRef::new(Uuid::new_v4(), "contact", "c-1")
}

#[test]
fn seal_then_open_round_trips() {
    let dek = Dek::generate();
    let s = subject();
    let sealed = seal(&dek, &s, b"alice@example.com").unwrap();
    assert_eq!(open(&dek, &s, &sealed).unwrap(), b"alice@example.com");
}

#[test]
fn sealed_output_starts_with_version_byte_and_is_not_plaintext() {
    let dek = Dek::generate();
    let sealed = seal(&dek, &subject(), b"secret subject line").unwrap();
    assert_eq!(sealed[0], ENVELOPE_VERSION_V1);
    assert_eq!(sealed.len(), 1 + NONCE_LEN + b"secret subject line".len() + TAG_LEN);
    assert!(!sealed.windows(6).any(|w| w == b"secret"));
}

#[test]
fn nonce_is_random_per_seal() {
    let dek = Dek::generate();
    let s = subject();
    assert_ne!(seal(&dek, &s, b"x").unwrap(), seal(&dek, &s, b"x").unwrap());
}

#[test]
fn ciphertext_cannot_be_moved_to_another_subject_even_with_the_same_dek() {
    let dek = Dek::generate();
    let tenant = Uuid::new_v4();
    let a = SubjectRef::new(tenant, "contact", "c-1");
    let b = SubjectRef::new(tenant, "contact", "c-2");
    let sealed = seal(&dek, &a, b"payload").unwrap();
    assert!(matches!(open(&dek, &b, &sealed), Err(CryptoError::DecryptFailed)));
}

#[test]
fn ciphertext_cannot_be_moved_to_another_tenant_even_with_the_same_dek() {
    let dek = Dek::generate();
    let a = SubjectRef::new(Uuid::new_v4(), "contact", "c-1");
    let b = SubjectRef::new(Uuid::new_v4(), "contact", "c-1");
    let sealed = seal(&dek, &a, b"payload").unwrap();
    assert!(matches!(open(&dek, &b, &sealed), Err(CryptoError::DecryptFailed)));
}

#[test]
fn wrong_dek_fails_with_decrypt_failed() {
    let s = subject();
    let sealed = seal(&Dek::generate(), &s, b"payload").unwrap();
    assert!(matches!(open(&Dek::generate(), &s, &sealed), Err(CryptoError::DecryptFailed)));
}

#[test]
fn tampered_ciphertext_fails() {
    let dek = Dek::generate();
    let s = subject();
    let mut sealed = seal(&dek, &s, b"payload").unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 0x01;
    assert!(matches!(open(&dek, &s, &sealed), Err(CryptoError::DecryptFailed)));
}

#[test]
fn unknown_version_byte_is_a_typed_error() {
    let dek = Dek::generate();
    let s = subject();
    let mut sealed = seal(&dek, &s, b"payload").unwrap();
    sealed[0] = 0x7F;
    assert!(matches!(open(&dek, &s, &sealed), Err(CryptoError::UnsupportedVersion(0x7F))));
}

#[test]
fn truncated_input_is_malformed() {
    let dek = Dek::generate();
    let s = subject();
    assert!(matches!(open(&dek, &s, &[]), Err(CryptoError::MalformedCiphertext)));
    assert!(matches!(
        open(&dek, &s, &[ENVELOPE_VERSION_V1, 1, 2, 3]),
        Err(CryptoError::MalformedCiphertext)
    ));
}

#[test]
fn empty_plaintext_round_trips() {
    let dek = Dek::generate();
    let s = subject();
    let sealed = seal(&dek, &s, b"").unwrap();
    assert_eq!(open(&dek, &s, &sealed).unwrap(), Vec::<u8>::new());
}

proptest! {
    #[test]
    fn open_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let dek = Dek::from_bytes([3; 32]);
        let s = SubjectRef::new(Uuid::nil(), "contact", "c-1");
        let _ = open(&dek, &s, &bytes);
    }

    #[test]
    fn open_never_panics_on_arbitrary_bytes_behind_a_valid_version(
        bytes in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        let dek = Dek::from_bytes([3; 32]);
        let s = SubjectRef::new(Uuid::nil(), "contact", "c-1");
        let mut input = vec![ENVELOPE_VERSION_V1];
        input.extend(bytes);
        prop_assert!(open(&dek, &s, &input).is_err());
    }
}
