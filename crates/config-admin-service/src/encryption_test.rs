use super::*;

fn sample_encryptor() -> ApiKeyEncryptor {
    ApiKeyEncryptor::new([7u8; 32])
}

#[test]
fn encrypt_then_decrypt_round_trips() {
    let encryptor = sample_encryptor();

    let ciphertext = encryptor.encrypt("sk-real-api-key-value").unwrap();
    let plaintext = encryptor.decrypt(&ciphertext).unwrap();

    assert_eq!(plaintext, "sk-real-api-key-value");
}

#[test]
fn ciphertext_never_contains_the_plaintext() {
    let encryptor = sample_encryptor();

    let ciphertext = encryptor.encrypt("sk-real-api-key-value").unwrap();

    assert!(!ciphertext.contains("sk-real-api-key-value"));
}

#[test]
fn encrypting_the_same_plaintext_twice_produces_different_ciphertext() {
    let encryptor = sample_encryptor();

    let a = encryptor.encrypt("same-value").unwrap();
    let b = encryptor.encrypt("same-value").unwrap();

    assert_ne!(a, b, "nonces must differ between encryptions");
}

#[test]
fn decrypting_with_the_wrong_key_fails() {
    let encryptor_a = ApiKeyEncryptor::new([1u8; 32]);
    let encryptor_b = ApiKeyEncryptor::new([2u8; 32]);

    let ciphertext = encryptor_a.encrypt("secret").unwrap();

    assert!(encryptor_b.decrypt(&ciphertext).is_err());
}

#[test]
fn decrypting_tampered_ciphertext_fails() {
    let encryptor = sample_encryptor();
    let mut ciphertext = encryptor.encrypt("secret").unwrap().into_bytes();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0xFF;
    let tampered = String::from_utf8_lossy(&ciphertext).to_string();

    assert!(encryptor.decrypt(&tampered).is_err());
}

#[test]
fn from_base64_parses_a_32_byte_key() {
    let encoded = base64_engine.encode([9u8; 32]);

    let encryptor = ApiKeyEncryptor::from_base64(&encoded).unwrap();

    let ciphertext = encryptor.encrypt("value").unwrap();
    assert_eq!(encryptor.decrypt(&ciphertext).unwrap(), "value");
}

#[test]
fn from_base64_rejects_a_key_of_the_wrong_length() {
    let encoded = base64_engine.encode([9u8; 16]);

    let result = ApiKeyEncryptor::from_base64(&encoded);

    assert!(matches!(result, Err(EncryptionError::InvalidKeyLength)));
}
