//! Field-level AEAD envelope: `version (1) || nonce (12) || AES-256-GCM ciphertext+tag`.
//!
//! The AAD is the version byte plus [`SubjectRef::aad`], so a ciphertext only opens for the
//! exact tenant and subject it was sealed for — copying it into another subject's row (even one
//! that happens to share a DEK) fails authentication.

#[path = "envelope_test.rs"]
#[cfg(test)]
mod envelope_test;

use crate::{CryptoError, Dek, SubjectRef};
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};

pub const ENVELOPE_VERSION_V1: u8 = 0x01;
pub const NONCE_LEN: usize = 12;
pub const TAG_LEN: usize = 16;

pub fn seal(dek: &Dek, subject: &SubjectRef, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(dek.as_bytes()));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let aad = aad_for(ENVELOPE_VERSION_V1, subject);
    let ciphertext = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad: &aad })
        .map_err(|_| CryptoError::EncryptFailed)?;
    let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    out.push(ENVELOPE_VERSION_V1);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn open(dek: &Dek, subject: &SubjectRef, envelope: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let (&version, rest) = envelope.split_first().ok_or(CryptoError::MalformedCiphertext)?;
    if version != ENVELOPE_VERSION_V1 {
        return Err(CryptoError::UnsupportedVersion(version));
    }
    if rest.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::MalformedCiphertext);
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(dek.as_bytes()));
    let aad = aad_for(version, subject);
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: &aad })
        .map_err(|_| CryptoError::DecryptFailed)
}

fn aad_for(version: u8, subject: &SubjectRef) -> Vec<u8> {
    let mut aad = vec![version];
    aad.extend(subject.aad());
    aad
}
